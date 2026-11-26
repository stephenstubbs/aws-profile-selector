use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Result, bail, ensure};
use rustix::fs::{FlockOperation, OFlags, flock};
use rustix::process::geteuid;
use tempfile::NamedTempFile;

use super::MAX_BYTES;

#[derive(Clone, Debug)]
pub struct Paths {
    pub home: PathBuf,
    pub directory: PathBuf,
    pub sources: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub bytes: Vec<u8>,
    identity: Option<(u64, u64)>,
    private: bool,
}

impl Snapshot {
    pub fn needs_protection(&self) -> bool {
        self.identity.is_some() && !self.private
    }
}

pub struct Lock {
    _file: File,
}

impl Paths {
    pub fn discover() -> Result<Self> {
        ensure!(
            geteuid().as_raw() != 0,
            "run AWS metadata management as your normal user, not root"
        );
        let home = std::env::home_dir()
            .ok_or_else(|| anyhow::anyhow!("HOME is required for metadata management"))?;
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let directory = config.join("aws-profile-selector");
        safe_path(&directory)?;
        Ok(Self {
            home,
            sources: directory.join("sources.json"),
            directory,
        })
    }

    pub fn target(&self, explicit: Option<&Path>) -> Result<PathBuf> {
        let default = self.home.join(".aws/config");
        let target = if let Some(path) = explicit {
            path.to_owned()
        } else {
            if let Some(path) = std::env::var_os("AWS_CONFIG_FILE").filter(|v| !v.is_empty()) {
                ensure!(
                    Path::new(&path) == default,
                    "AWS_CONFIG_FILE selects another destination; use an explicit --target outside a workspace"
                );
            }
            default
        };
        safe_path(&target)?;
        Ok(target)
    }
}

pub fn safe_path(path: &Path) -> Result<()> {
    ensure!(
        path.is_absolute() && !path.components().any(|part| part == Component::ParentDir),
        "metadata paths must be absolute and contain no parent traversal"
    );
    ensure!(
        !in_workspace(path),
        "metadata must not be stored in a workspace"
    );
    let mut existing = path;
    while fs::symlink_metadata(existing).is_err() {
        existing = existing
            .parent()
            .ok_or_else(|| anyhow::anyhow!("invalid metadata path"))?;
    }
    let physical = fs::canonicalize(existing)
        .map_err(|_| anyhow::anyhow!("cannot resolve metadata destination"))?;
    ensure!(
        !in_workspace(&physical),
        "metadata must not resolve into a workspace"
    );
    ensure!(
        !physical.starts_with("/nix/store"),
        "metadata must not be stored in the Nix store"
    );
    let directory = if physical.is_dir() {
        physical.as_path()
    } else {
        physical
            .parent()
            .ok_or_else(|| anyhow::anyhow!("invalid metadata parent"))?
    };
    for parent in directory.ancestors() {
        let meta = fs::metadata(parent)
            .map_err(|_| anyhow::anyhow!("cannot inspect metadata ancestors"))?;
        ensure!(
            meta.uid() == 0 || meta.uid() == geteuid().as_raw(),
            "metadata parent has a foreign owner"
        );
        ensure!(
            meta.mode() & 0o022 == 0 || meta.mode() & 0o1000 != 0,
            "metadata parent is writable by another user"
        );
        if meta.uid() == geteuid().as_raw() && meta.mode() & 0o077 == 0 {
            break;
        }
    }
    Ok(())
}

fn in_workspace(path: &Path) -> bool {
    path.ancestors()
        .any(|parent| parent.join(".git").exists() || parent.join(".jj").exists())
}

pub fn private_directory(path: &Path) -> Result<()> {
    safe_path(path)?;
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_dir() && !meta.file_type().is_symlink(),
                "private metadata directory is not a real directory"
            );
            ensure!(
                meta.uid() == geteuid().as_raw(),
                "private metadata directory has a foreign owner"
            );
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))
                .map_err(|_| anyhow::anyhow!("cannot protect metadata directory"))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| anyhow::anyhow!("invalid metadata directory"))?;
            if !parent.exists() {
                private_directory(parent)?;
            }
            fs::DirBuilder::new()
                .mode(0o700)
                .create(path)
                .map_err(|_| anyhow::anyhow!("cannot create metadata directory"))?;
        }
        Err(_) => bail!("cannot inspect metadata directory"),
    }
    Ok(())
}

pub fn snapshot(path: &Path, private: bool) -> Result<Snapshot> {
    safe_path(path)?;
    let flags = OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(flags.bits() as i32)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Snapshot {
                bytes: Vec::new(),
                identity: None,
                private: true,
            });
        }
        Err(_) => bail!("cannot open a regular metadata file"),
    };
    let meta = file
        .metadata()
        .map_err(|_| anyhow::anyhow!("cannot inspect metadata file"))?;
    ensure!(
        meta.is_file() && meta.nlink() == 1,
        "metadata must be a single-link regular file"
    );
    ensure!(
        meta.uid() == geteuid().as_raw(),
        "metadata file has a foreign owner"
    );
    ensure!(
        meta.mode() & 0o022 == 0,
        "metadata file is writable by another user"
    );
    ensure!(
        !private || meta.mode() & 0o077 == 0,
        "source registry must have private permissions"
    );
    ensure!(meta.len() <= MAX_BYTES as u64, "metadata file is oversized");
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| anyhow::anyhow!("cannot read metadata file"))?;
    ensure!(bytes.len() <= MAX_BYTES, "metadata file is oversized");
    let current = fs::symlink_metadata(path)
        .map_err(|_| anyhow::anyhow!("metadata changed during reading"))?;
    ensure!(
        !current.file_type().is_symlink()
            && (current.dev(), current.ino()) == (meta.dev(), meta.ino()),
        "metadata changed during reading"
    );
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid metadata parent"))?;
    let directory = fs::metadata(parent)?;
    Ok(Snapshot {
        bytes,
        identity: Some((meta.dev(), meta.ino())),
        private: meta.mode() & 0o077 == 0 && directory.mode() & 0o077 == 0,
    })
}

pub fn atomic_write(path: &Path, expected: &Snapshot, contents: &[u8]) -> Result<()> {
    ensure!(
        contents.len() <= MAX_BYTES,
        "generated metadata is oversized"
    );
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid metadata path"))?;
    private_directory(parent)?;
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|_| anyhow::anyhow!("cannot stage private metadata"))?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o600))?;
    temporary
        .write_all(contents)
        .map_err(|_| anyhow::anyhow!("cannot stage metadata"))?;
    temporary
        .as_file()
        .sync_all()
        .map_err(|_| anyhow::anyhow!("cannot sync staged metadata"))?;
    let current = snapshot(path, false)?;
    ensure!(
        current.identity == expected.identity && current.bytes == expected.bytes,
        "metadata changed concurrently; no replacement was installed"
    );
    if expected.identity.is_none() {
        temporary
            .persist_noclobber(path)
            .map_err(|_| anyhow::anyhow!("metadata destination appeared concurrently"))?;
    } else {
        temporary
            .persist(path)
            .map_err(|_| anyhow::anyhow!("cannot atomically install metadata"))?;
    }
    File::open(parent)?
        .sync_all()
        .map_err(|_| anyhow::anyhow!("cannot sync metadata directory"))?;
    Ok(())
}

pub fn lock(directory: &Path, timeout: Duration) -> Result<Lock> {
    private_directory(directory)?;
    let path = directory.join(".lock");
    let flags = OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(flags.bits() as i32)
        .open(&path)
        .map_err(|_| anyhow::anyhow!("cannot open metadata lock"))?;
    let meta = file.metadata()?;
    ensure!(
        meta.is_file()
            && meta.nlink() == 1
            && meta.uid() == geteuid().as_raw()
            && meta.mode() & 0o077 == 0,
        "metadata lock is not private"
    );
    let deadline = Instant::now() + timeout;
    loop {
        match flock(&file, FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {
                let current = fs::symlink_metadata(&path)?;
                ensure!(
                    !current.file_type().is_symlink()
                        && (current.dev(), current.ino()) == (meta.dev(), meta.ino()),
                    "metadata lock was replaced"
                );
                return Ok(Lock { _file: file });
            }
            Err(rustix::io::Errno::WOULDBLOCK) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => bail!("metadata lock is busy or unavailable"),
        }
    }
}
