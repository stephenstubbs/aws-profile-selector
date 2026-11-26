use std::fs;
use std::process::{Command, Output};

use serde_json::{Value, json};
use tempfile::{TempDir, tempdir};

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Self {
        let result = Self(tempdir().unwrap());
        fs::create_dir(result.0.path().join(".aws")).unwrap();
        fs::write(
            result.0.path().join(".aws/config"),
            "[profile fixture-a]\nregion=eu-west-1\n[profile fixture-b]\nregion=eu-west-2\n",
        )
        .unwrap();
        result
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aws-profile-selector"));
        command
            .env_clear()
            .env("HOME", self.0.path())
            .env("SHELL", "/bin/sh")
            .current_dir(self.0.path());
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{:?}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn saved(&self) -> String {
        fs::read_to_string(self.0.path().join(".aws/current-profile")).unwrap()
    }
}

#[test]
fn structured_selection_is_emitted_only_after_atomic_persistence() {
    let fixture = Fixture::new();
    assert_eq!(
        fixture.json(&["--show-current", "--json"]),
        json!({"profile":null})
    );
    assert_eq!(
        fixture.json(&["-a", "fixture-a", "--json"]),
        json!({"profile":"fixture-a"})
    );
    assert_eq!(fixture.saved(), "fixture-a");
    assert_eq!(
        fixture.json(&["-ca", "fixture-b", "--context", "--json"]),
        json!({"profile":"fixture-b"})
    );
    assert_eq!(fixture.saved(), "fixture-a");
    assert_eq!(fixture.json(&["-cd", "--json"]), json!({"profile":null}));
    assert_eq!(fixture.saved(), "fixture-a");
    assert_eq!(fixture.json(&["-d", "--json"]), json!({"profile":null}));
    assert!(!fixture.0.path().join(".aws/current-profile").exists());
}

#[test]
fn failed_context_actions_emit_nothing_and_keep_the_saved_selection() {
    let fixture = Fixture::new();
    fixture.json(&["-a", "fixture-a", "--json"]);
    let output = fixture.run(&["-a", "missing", "--context", "--json"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert_eq!(fixture.saved(), "fixture-a");
    let path = fixture.0.path().join(".aws/current-profile");
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    let output = fixture.run(&["-n", "fixture-b", "--json"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(path.is_dir());
}

#[test]
fn context_commands_round_trip_literal_values_without_evaluating_them() {
    let fixture = Fixture::new();
    let profile = r#"team's "$dev" $(touch injected) `id`"#;
    let output = fixture.run(&["-n", profile, "--context", "--shell", "posix"]);
    assert!(output.status.success());
    let code = String::from_utf8(output.stdout).unwrap();
    let checked = Command::new("sh")
        .args(["-c", &format!("{code}\nprintf '%s' \"$AWS_PROFILE\"")])
        .current_dir(fixture.0.path())
        .output()
        .unwrap();
    assert!(checked.status.success());
    assert_eq!(String::from_utf8(checked.stdout).unwrap(), profile);
    assert_eq!(fixture.saved(), profile);
    assert!(!fixture.0.path().join("injected").exists());
}

#[test]
fn credential_environment_cannot_silently_override_a_profile_switch() {
    let fixture = Fixture::new();
    fixture.json(&["-a", "fixture-a", "--json"]);
    for key in [
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_WEB_IDENTITY_TOKEN_FILE",
        "AWS_ROLE_ARN",
        "AWS_CONTAINER_CREDENTIALS_FULL_URI",
        "AWS_DEFAULT_PROFILE",
    ] {
        let output = fixture
            .command()
            .env(key, "fictional-credential-canary")
            .args(["-a", "fixture-b", "--json"])
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains(key));
        assert!(!error.contains("fictional-credential-canary"));
        assert_eq!(fixture.saved(), "fixture-a");
    }
    let output = fixture
        .command()
        .env("AWS_ACCESS_KEY_ID", "fictional-credential-canary")
        .args(["-d", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn saved_selection_reads_are_bounded_regular_files_and_never_read_native_credentials() {
    let fixture = Fixture::new();
    fs::write(fixture.0.path().join(".aws/credentials"), [0xff, 0xfe]).unwrap();
    assert_eq!(
        fixture.json(&["--show-current", "--json"]),
        json!({"profile":null})
    );
    let path = fixture.0.path().join(".aws/current-profile");
    fs::write(&path, "bad\u{1b}profile").unwrap();
    let output = fixture.run(&["--show-current", "--json"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(fixture.0.path().join(".aws/config"), &path).unwrap();
    assert!(!fixture.run(&["--show-current", "--json"]).status.success());
}

#[test]
fn metadata_commands_cannot_be_mixed_with_selection_or_listing() {
    let fixture = Fixture::new();
    for args in [
        vec!["--json", "config", "status"],
        vec!["-a", "fixture-a", "config", "status"],
        vec!["--list", "--json"],
        vec!["--show-current", "-c"],
    ] {
        assert_eq!(fixture.run(&args).status.code(), Some(2));
    }
    assert!(!fixture.0.path().join(".aws/current-profile").exists());
}
