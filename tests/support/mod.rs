use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::{TempDir, tempdir};

pub const ACCOUNT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const VAULT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbb";
pub const DOCUMENT: &str = "cccccccccccccccccccccccccc";

pub struct Fixture {
    pub root: TempDir,
}

impl Fixture {
    pub fn new() -> Self {
        Self {
            root: tempdir().unwrap(),
        }
    }

    pub fn path(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    pub fn write(&self, name: &str, text: &str) -> PathBuf {
        let path = self.path(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        path
    }

    pub fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aws-profile-selector"));
        command
            .env_clear()
            .env("HOME", self.root.path())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("AWS_PROFILE_SELECTOR_OP_EXECUTABLE", self.path("op"))
            .current_dir(self.root.path());
        command
    }

    pub fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    pub fn bind(&self, name: &str, document: &str) {
        success(self.run(&[
            "config",
            "source",
            "add",
            name,
            "--account",
            ACCOUNT,
            "--vault",
            VAULT,
            "--document",
            document,
        ]));
    }

    pub fn op(&self, body: &str) -> PathBuf {
        let path = self.write("op", &format!("#!/bin/sh\n{body}\n"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
        path
    }

    pub fn catalog(&self) -> PathBuf {
        self.write(
            "catalog.ini",
            "[profile fixture]\nsso_session=fixture-login\nsso_account_id=111111111111\n\
             sso_role_name=Developer\nregion=eu-west-1\n\
             [sso-session fixture-login]\nsso_start_url=https://example.invalid/start\n\
             sso_region=eu-west-1\n",
        )
    }

    pub fn json(output: Output) -> Value {
        serde_json::from_str(&success(output)).unwrap()
    }
}

pub fn success(output: Output) -> String {
    assert!(
        output.status.success(),
        "{:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
