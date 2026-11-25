use std::{env, fs, io::ErrorKind, io::Write, path::PathBuf};

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

fn current_profile_path() -> Result<PathBuf> {
    Ok(env::home_dir()
        .context("Unable to determine home directory")?
        .join(".aws/current-profile"))
}

pub fn activate(name: &str) -> Result<()> {
    let path = current_profile_path()?;
    let parent = path.parent().context("Invalid current-profile path")?;
    fs::create_dir_all(parent).with_context(|| format!("Failed to create {}", parent.display()))?;
    let mut file = NamedTempFile::new_in(parent)
        .with_context(|| format!("Failed to create a profile file in {}", parent.display()))?;
    file.write_all(name.as_bytes())
        .context("Failed to write selected profile")?;
    file.persist(&path)
        .with_context(|| format!("Failed to save selected profile to {}", path.display()))?;
    Ok(())
}

pub fn deactivate() -> Result<bool> {
    let path = current_profile_path()?;
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("Failed to remove {}", path.display())),
    }
}
