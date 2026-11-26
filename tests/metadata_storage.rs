use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
use std::time::Duration;

use aws_profile_selector::metadata::storage;
use tempfile::tempdir;

#[test]
fn atomic_install_is_private_and_refuses_concurrent_changes() {
    let root = tempdir().unwrap();
    let path = root.path().join("state/config");
    let before = storage::snapshot(&path, false).unwrap();
    storage::atomic_write(&path, &before, b"first").unwrap();
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    let snapshot = storage::snapshot(&path, false).unwrap();
    fs::write(&path, b"concurrent").unwrap();
    assert!(storage::atomic_write(&path, &snapshot, b"replacement").is_err());
    assert_eq!(fs::read(&path).unwrap(), b"concurrent");
}

#[test]
fn refuses_symlink_hardlink_and_workspace_destinations() {
    let root = tempdir().unwrap();
    let path = root.path().join("config");
    let target = root.path().join("unrelated");
    fs::write(&target, b"canary").unwrap();
    symlink(&target, &path).unwrap();
    assert!(storage::snapshot(&path, false).is_err());
    fs::remove_file(&path).unwrap();
    fs::hard_link(&target, &path).unwrap();
    assert!(storage::snapshot(&path, false).is_err());
    fs::create_dir(root.path().join(".jj")).unwrap();
    assert!(storage::safe_path(&root.path().join("new/config")).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"canary");
}

#[test]
fn lock_is_bounded_and_foreign_modes_are_never_rewritten() {
    let root = tempdir().unwrap();
    let _held = storage::lock(root.path(), Duration::from_secs(1)).unwrap();
    assert!(storage::lock(root.path(), Duration::from_millis(50)).is_err());
    drop(_held);
    fs::set_permissions(root.path().join(".lock"), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(storage::lock(root.path(), Duration::from_millis(1)).is_err());
    assert_eq!(
        fs::metadata(root.path().join(".lock")).unwrap().mode() & 0o777,
        0o644
    );
}

#[test]
fn source_registry_permissions_are_stricter_than_existing_native_configuration() {
    let root = tempdir().unwrap();
    let path = root.path().join("sources.json");
    fs::write(&path, b"{}").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(storage::snapshot(&path, true).is_err());
    assert!(storage::snapshot(&path, false).is_ok());
}

#[test]
fn a_private_boundary_does_not_allow_writable_children_or_workspace_paths() {
    let root = tempdir().unwrap();
    let writable = root.path().join("writable");
    fs::create_dir(&writable).unwrap();
    fs::set_permissions(&writable, fs::Permissions::from_mode(0o777)).unwrap();
    assert!(storage::safe_path(&writable.join("config")).is_err());
    fs::set_permissions(&writable, fs::Permissions::from_mode(0o700)).unwrap();
    assert!(storage::safe_path(&writable.join("config")).is_ok());
    fs::create_dir(root.path().join(".git")).unwrap();
    assert!(storage::safe_path(&writable.join("config")).is_err());
}
