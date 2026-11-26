use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, ensure};
use clap::Subcommand;
use serde::Serialize;

use super::catalog::Catalog;
use super::fetch::{self, Cancellation};
use super::install;
use super::sources::{Registry, Source};
use super::storage::{self, Paths};

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Bind a metadata Document, or remove a binding (never an authentication session)
    Source {
        #[command(subcommand)]
        command: SourceCommand,
    },
    /// Retrieve all bound SSO catalogs and atomically update the managed config section
    Sync {
        #[arg(long)]
        target: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        adopt: bool,
        #[arg(long)]
        accept_identity_change: bool,
        #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=120))]
        timeout: u64,
    },
    /// Validate a private SSO catalog without authentication or configuration writes
    Validate {
        #[arg(long)]
        file: PathBuf,
    },
    /// Install an explicitly supplied private SSO catalog without contacting 1Password
    Install {
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        target: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        adopt: bool,
        #[arg(long)]
        accept_identity_change: bool,
    },
    /// Remove only the managed SSO metadata; login caches and other settings stay intact
    Clear {
        #[arg(long)]
        target: Option<PathBuf>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Show metadata counts only; never read 1Password or authentication caches
    Status {
        #[arg(long)]
        target: Option<PathBuf>,
    },
}

#[derive(Debug, Subcommand)]
pub enum SourceCommand {
    Add {
        name: String,
        #[arg(long)]
        account: String,
        #[arg(long)]
        vault: String,
        #[arg(long)]
        document: String,
        #[arg(long)]
        replace: bool,
    },
    Remove {
        name: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub sources: usize,
    pub profiles: usize,
    pub sessions: usize,
    pub changed: bool,
    pub dry_run: bool,
}

fn text(bytes: &[u8]) -> Result<&str> {
    std::str::from_utf8(bytes).map_err(|_| anyhow::anyhow!("metadata is not UTF-8"))
}

fn read_catalog(path: &Path) -> Result<Catalog> {
    Catalog::parse(text(&storage::snapshot(path, true)?.bytes)?)
}

fn report(catalog: &Catalog, sources: usize, changed: bool, dry_run: bool) -> Report {
    Report {
        sources,
        profiles: catalog.profile_count(),
        sessions: catalog.session_count(),
        changed,
        dry_run,
    }
}

fn output(report: &Report) -> Result<()> {
    println!("{}", serde_json::to_string(report)?);
    Ok(())
}

pub fn execute(command: ConfigCommand) -> Result<()> {
    let paths = Paths::discover()?;
    match command {
        ConfigCommand::Source { command } => {
            if let SourceCommand::Add {
                name,
                account,
                vault,
                document,
                ..
            } = &command
            {
                Registry {
                    schema_version: 1,
                    sources: vec![Source {
                        name: name.clone(),
                        account: account.clone(),
                        vault: vault.clone(),
                        document: document.clone(),
                    }],
                }
                .validate()?;
            }
            let _lock = storage::lock(&paths.directory, Duration::from_secs(5))?;
            let (mut registry, before) = Registry::load(&paths)?;
            match command {
                SourceCommand::Add {
                    name,
                    account,
                    vault,
                    document,
                    replace,
                } => {
                    registry.add(
                        Source {
                            name,
                            account,
                            vault,
                            document,
                        },
                        replace,
                    )?;
                }
                SourceCommand::Remove { name } => registry.remove(&name)?,
            }
            registry.save(&paths, &before)?;
            output(&report(
                &Catalog::default(),
                registry.sources.len(),
                true,
                false,
            ))
        }
        ConfigCommand::Validate { file } => output(&report(&read_catalog(&file)?, 0, false, true)),
        ConfigCommand::Install {
            file,
            target,
            dry_run,
            adopt,
            accept_identity_change,
        } => {
            let catalog = read_catalog(&file)?;
            let target = paths.target(target.as_deref())?;
            let before = storage::snapshot(&target, false)?;
            let merged = if accept_identity_change {
                install::merge_with_identity_approval(text(&before.bytes)?, &catalog, adopt)?
            } else {
                install::merge(text(&before.bytes)?, &catalog, adopt)?
            };
            let changed = merged.as_bytes() != before.bytes || before.needs_protection();
            if !dry_run && changed {
                let _lock = storage::lock(&paths.directory, Duration::from_secs(5))?;
                storage::atomic_write(&target, &before, merged.as_bytes())?;
            }
            output(&report(&catalog, 0, changed, dry_run))
        }
        ConfigCommand::Sync {
            target,
            dry_run,
            adopt,
            accept_identity_change,
            timeout,
        } => {
            let (registry, registry_before) = Registry::load(&paths)?;
            ensure!(
                !registry.sources.is_empty(),
                "no metadata sources configured; bind a 1Password Document first"
            );
            let target = paths.target(target.as_deref())?;
            let before = storage::snapshot(&target, false)?;
            let cancellation = Cancellation::new()?;
            let overall = fetch::overall_deadline();
            let mut catalog = Catalog::default();
            for source in &registry.sources {
                let deadline = (Instant::now() + Duration::from_secs(timeout)).min(overall);
                let payload = fetch::document(
                    &fetch::default_executable(),
                    source,
                    deadline,
                    &cancellation,
                )?;
                catalog.merge(Catalog::parse(text(&payload)?)?)?;
            }
            let merged = if accept_identity_change {
                install::merge_with_identity_approval(text(&before.bytes)?, &catalog, adopt)?
            } else {
                install::merge(text(&before.bytes)?, &catalog, adopt)?
            };
            let changed = merged.as_bytes() != before.bytes || before.needs_protection();
            cancellation.check()?;
            if !dry_run {
                let _lock = storage::lock(&paths.directory, Duration::from_secs(5))?;
                let (_, current) = Registry::load(&paths)?;
                ensure!(
                    current.bytes == registry_before.bytes,
                    "source bindings changed during retrieval; no metadata was installed"
                );
                cancellation.check()?;
                if changed {
                    storage::atomic_write(&target, &before, merged.as_bytes())?;
                } else {
                    ensure!(
                        storage::snapshot(&target, false)?.bytes == before.bytes,
                        "configuration changed during retrieval; retry metadata sync"
                    );
                }
            }
            output(&report(&catalog, registry.sources.len(), changed, dry_run))
        }
        ConfigCommand::Clear { target, dry_run } => {
            let target = paths.target(target.as_deref())?;
            let before = storage::snapshot(&target, false)?;
            let cleared = install::clear(text(&before.bytes)?)?;
            let changed = cleared.as_bytes() != before.bytes;
            if !dry_run && changed {
                let _lock = storage::lock(&paths.directory, Duration::from_secs(5))?;
                storage::atomic_write(&target, &before, cleared.as_bytes())?;
            }
            output(&report(&Catalog::default(), 0, changed, dry_run))
        }
        ConfigCommand::Status { target } => {
            let (registry, _) = Registry::load(&paths)?;
            let target = paths.target(target.as_deref())?;
            let config = storage::snapshot(&target, false)?;
            let (_, managed) = install::split_managed(text(&config.bytes)?)?;
            output(&report(
                &managed.unwrap_or_default(),
                registry.sources.len(),
                false,
                false,
            ))
        }
    }
}
