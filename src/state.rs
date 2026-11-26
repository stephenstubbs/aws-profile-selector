use std::{env, fs, io::ErrorKind, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use aws_profile_selector::metadata::storage;
use tempfile::NamedTempFile;

fn current_profile_path() -> Result<PathBuf> {
    Ok(env::home_dir()
        .context("Unable to determine home directory")?
        .join(".aws/current-profile"))
}

pub fn activate(name: &str) -> Result<()> {
    let path = current_profile_path()?;
    let parent = path.parent().context("Invalid current-profile path")?;
    storage::private_directory(parent)?;
    let mut file = NamedTempFile::new_in(parent)
        .with_context(|| format!("Failed to create a profile file in {}", parent.display()))?;
    file.write_all(name.as_bytes())
        .context("Failed to write selected profile")?;
    file.persist(&path)
        .with_context(|| format!("Failed to save selected profile to {}", path.display()))?;
    Ok(())
}

pub fn current() -> Result<Option<String>> {
    let path = current_profile_path()?;
    let snapshot = storage::snapshot(&path, false)?;
    if snapshot.bytes.is_empty() {
        return Ok(None);
    }
    anyhow::ensure!(
        snapshot.bytes.len() <= 4096,
        "saved AWS selection is oversized"
    );
    let name = std::str::from_utf8(&snapshot.bytes)
        .map_err(|_| anyhow::anyhow!("saved AWS selection is not UTF-8"))?
        .trim_end_matches(['\r', '\n']);
    crate::cli::validate_profile_name(name).map_err(anyhow::Error::msg)?;
    Ok(Some(name.to_owned()))
}

pub fn deactivate() -> Result<bool> {
    let path = current_profile_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("Failed to remove {}", path.display())),
    }
}
