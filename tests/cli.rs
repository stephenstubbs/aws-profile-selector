use std::{fs, process::Command, process::Output};

use tempfile::{TempDir, tempdir};

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Self {
        Self(tempdir().unwrap())
    }

    fn command(&self) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aws-profile-selector"));
        command
            .env_clear()
            .env("HOME", self.0.path())
            .env("USERPROFILE", self.0.path())
            .env("SHELL", "/bin/sh")
            .current_dir(self.0.path());
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn config(&self, content: &str) {
        fs::create_dir_all(self.0.path().join(".aws")).unwrap();
        fs::write(self.0.path().join(".aws/config"), content).unwrap();
    }

    fn saved_profile(&self) -> Option<String> {
        fs::read_to_string(self.0.path().join(".aws/current-profile")).ok()
    }
}

fn success(output: Output) -> String {
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn help_and_version_do_not_require_configuration() {
    let fixture = Fixture::new();
    let help = success(fixture.run(&["--help"]));
    assert!(help.contains("--activate"));
    assert!(help.contains("--shell"));
    assert_eq!(
        success(fixture.run(&["--version"])),
        format!("aws-profile-selector {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(!fixture.0.path().join(".aws").exists());
}

#[test]
fn new_profile_is_saved_and_replaced_without_leaving_temporary_files() {
    let fixture = Fixture::new();
    success(fixture.run(&["-n", "dev"]));
    assert_eq!(fixture.saved_profile().as_deref(), Some("dev"));
    success(fixture.run(&["-n", "prod"]));
    assert_eq!(fixture.saved_profile().as_deref(), Some("prod"));
    assert_eq!(
        fs::read_dir(fixture.0.path().join(".aws")).unwrap().count(),
        1
    );
}

#[test]
fn deactivation_is_idempotent_and_does_not_require_config() {
    let fixture = Fixture::new();
    success(fixture.run(&["-d"]));
    assert!(!fixture.0.path().join(".aws").exists());
    success(fixture.run(&["-n", "dev"]));
    success(fixture.run(&["-d"]));
    assert_eq!(fixture.saved_profile(), None);
    success(fixture.run(&["-d"]));
}

#[test]
fn current_mode_never_changes_saved_state() {
    let fixture = Fixture::new();
    assert_eq!(
        success(fixture.run(&["-cn", "dev", "--shell", "nu"])),
        "$env.AWS_PROFILE = \"dev\"\n"
    );
    assert!(!fixture.0.path().join(".aws").exists());
    success(fixture.run(&["-n", "saved"]));
    assert_eq!(
        success(fixture.run(&["-cd", "--shell", "nu"])),
        "hide-env --ignore-errors AWS_PROFILE\n"
    );
    assert_eq!(fixture.saved_profile().as_deref(), Some("saved"));
}

#[test]
fn activates_default_config_and_credentials_only_profiles() {
    let fixture = Fixture::new();
    fixture.config(
        "[default]\nregion=us-east-1\n[profile dev]\nregion=us-west-2\n\
         [sso-session login]\nsso_region=eu-west-1\n",
    );
    fs::write(
        fixture.0.path().join(".aws/credentials"),
        "[legacy]\naws_access_key_id=not-a-real-key\n",
    )
    .unwrap();
    for name in ["default", "dev", "legacy"] {
        success(fixture.run(&["-a", name]));
        assert_eq!(fixture.saved_profile().as_deref(), Some(name));
    }
}

#[test]
fn honors_relative_configuration_overrides() {
    let fixture = Fixture::new();
    fs::write(
        fixture.0.path().join("custom-config"),
        "[profile configured]\n",
    )
    .unwrap();
    fs::write(
        fixture.0.path().join("custom-credentials"),
        "[credentials-only]\n",
    )
    .unwrap();
    for name in ["configured", "credentials-only"] {
        let output = fixture
            .command()
            .env("AWS_CONFIG_FILE", "custom-config")
            .env("AWS_SHARED_CREDENTIALS_FILE", "custom-credentials")
            .args(["-a", name])
            .output()
            .unwrap();
        success(output);
        assert_eq!(fixture.saved_profile().as_deref(), Some(name));
    }
}

#[test]
fn missing_override_is_reported_without_changing_saved_profile() {
    let fixture = Fixture::new();
    success(fixture.run(&["-n", "saved"]));
    let output = fixture
        .command()
        .env("AWS_CONFIG_FILE", "missing-config")
        .args(["-a", "dev"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("missing-config")
    );
    assert!(output.stdout.is_empty());
    assert_eq!(fixture.saved_profile().as_deref(), Some("saved"));
}

#[test]
fn invalid_activation_lists_profiles_on_stderr_only() {
    let fixture = Fixture::new();
    fixture.config("[profile prod]\n[profile dev]\n");
    let output = fixture.run(&["-ca", "missing"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let error = String::from_utf8(output.stderr).unwrap();
    assert!(error.contains("Available profiles: dev, prod"));
    assert_eq!(fixture.saved_profile(), None);
}

#[test]
fn empty_configuration_fails_without_prompting() {
    let fixture = Fixture::new();
    let output = fixture.run(&[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("No AWS profiles found")
    );
}

#[test]
fn shell_detection_uses_basename_and_can_be_overridden() {
    let fixture = Fixture::new();
    for (shell, expected) in [
        ("/nuclear/bin/bash", "export AWS_PROFILE='dev'\n"),
        ("/opt/bin/nu", "$env.AWS_PROFILE = \"dev\"\n"),
        ("/opt/bin/fish", "set -gx AWS_PROFILE 'dev'\n"),
    ] {
        assert_eq!(
            success(
                fixture
                    .command()
                    .env("SHELL", shell)
                    .args(["-cn", "dev"])
                    .output()
                    .unwrap()
            ),
            expected
        );
    }
    assert_eq!(
        success(
            fixture
                .command()
                .env("SHELL", "/bin/fish")
                .args(["-cn", "dev", "--shell", "bash"])
                .output()
                .unwrap()
        ),
        "export AWS_PROFILE='dev'\n"
    );
}

#[cfg(unix)]
#[test]
fn posix_commands_round_trip_without_executing_profile_contents() {
    let fixture = Fixture::new();
    let name = r#"team's "$dev" \ (prod); $(touch injected); `whoami`"#;
    let command = success(fixture.run(&["-cn", name, "--shell", "posix"]));
    let output = Command::new("sh")
        .args(["-c", &format!("{command}printf '%s' \"$AWS_PROFILE\"")])
        .current_dir(fixture.0.path())
        .output()
        .unwrap();
    assert_eq!(success(output), name);
    assert!(!fixture.0.path().join("injected").exists());
}

#[test]
fn conflicting_actions_and_invalid_names_fail_before_writing() {
    let fixture = Fixture::new();
    for args in [
        vec!["-n", "dev", "-d"],
        vec!["-a", "prod", "-n", "dev"],
        vec!["-n", ""],
        vec!["-n", "dev\nprod"],
    ] {
        assert_eq!(fixture.run(&args).status.code(), Some(2));
    }
    assert!(!fixture.0.path().join(".aws").exists());
}

#[test]
fn failed_persistence_is_contextual_and_cleans_up_temporary_files() {
    let fixture = Fixture::new();
    let path = fixture.0.path().join(".aws/current-profile");
    fs::create_dir_all(&path).unwrap();
    let output = fixture.run(&["-n", "dev"]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("Failed to save selected profile")
    );
    assert!(path.is_dir());
    assert_eq!(
        fs::read_dir(fixture.0.path().join(".aws")).unwrap().count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn activation_replaces_symlink_without_overwriting_its_target() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.path().join(".aws")).unwrap();
    let target = fixture.0.path().join("unrelated");
    fs::write(&target, "untouched").unwrap();
    std::os::unix::fs::symlink(&target, fixture.0.path().join(".aws/current-profile")).unwrap();
    success(fixture.run(&["-n", "dev"]));
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
    assert_eq!(fixture.saved_profile().as_deref(), Some("dev"));
}
