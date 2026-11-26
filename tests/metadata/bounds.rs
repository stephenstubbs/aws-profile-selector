use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::support::{DOCUMENT, Fixture, success};

#[test]
fn oversized_stdout_and_stderr_are_bounded_and_never_published() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    let large = fixture.path("large");
    fs::write(&large, vec![b'x'; 1024 * 1024 + 32]).unwrap();
    for script in [
        format!("cat '{}'", large.display()),
        format!("cat '{}' >&2; sleep 20", large.display()),
    ] {
        fixture.op(&script);
        let started = Instant::now();
        let result = fixture.run(&["config", "sync", "--timeout", "3"]);
        assert!(!result.status.success());
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!fixture.path(".aws/config").exists());
        assert!(result.stderr.len() < 1024);
    }
}

#[test]
fn cancellation_reaps_the_owned_backend_and_keeps_last_good_configuration() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    fixture.write(".aws/config", "[profile unchanged]\nregion=us-east-1\n");
    fixture.op(&format!(
        "printf '%s' \"$$\" > '{}'\nsleep 30",
        fixture.path("backend-pid").display()
    ));
    let mut command = fixture.command();
    command
        .args(["config", "sync", "--timeout", "10"])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fixture.path("backend-pid").exists() {
        assert!(Instant::now() < deadline, "backend did not become ready");
        assert!(
            child.try_wait().unwrap().is_none(),
            "metadata operation exited early"
        );
        thread::sleep(Duration::from_millis(10));
    }
    let backend = fs::read_to_string(fixture.path("backend-pid")).unwrap();
    assert!(
        Command::new("sh")
            .args(["-c", "kill -TERM \"$1\"", "_", &child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "cancellation did not complete");
        thread::sleep(Duration::from_millis(10));
    };
    assert!(!status.success());
    assert!(
        Command::new("sh")
            .args(["-c", "! kill -0 \"$1\" 2>/dev/null", "_", &backend])
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        fs::read_to_string(fixture.path(".aws/config")).unwrap(),
        "[profile unchanged]\nregion=us-east-1\n"
    );
}

#[test]
fn leader_exit_does_not_bypass_the_deadline_or_leave_a_descendant_writer() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    fixture.op(&format!(
        "(sleep 2; printf late > '{}') &\nexit 0",
        fixture.path("late-write").display()
    ));
    let result = fixture.run(&["config", "sync", "--timeout", "1"]);
    assert!(!result.status.success());
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        assert!(!fixture.path("late-write").exists());
        thread::sleep(Duration::from_millis(25));
    }
    assert!(!fixture.path(".aws/config").exists());
}

#[test]
fn invalid_registry_fields_and_duplicate_sources_are_closed_without_value_leaks() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    let path = fixture.path(".config/aws-profile-selector/sources.json");
    let original = fs::read_to_string(&path).unwrap();
    let mut data: serde_json::Value = serde_json::from_str(&original).unwrap();
    data["sources"][0]["secretToken"] = "fictional-registry-canary".into();
    fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    let output = fixture.run(&["config", "status"]);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fictional-registry-canary"));
    let mut data: serde_json::Value = serde_json::from_str(&original).unwrap();
    let duplicate = data["sources"][0].clone();
    data["sources"].as_array_mut().unwrap().push(duplicate);
    fs::write(&path, serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(!fixture.run(&["config", "sync"]).status.success());
    assert!(!fixture.path(".aws/config").exists());
}

#[test]
fn sync_dry_run_fetches_metadata_but_does_not_install_or_modify_selection() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    let catalog = fixture.catalog();
    fixture.op(&format!("cat '{}'", catalog.display()));
    fixture.write(".aws/current-profile", "local-selection");
    let report = success(fixture.run(&["config", "sync", "--dry-run"]));
    assert!(report.contains("\"dryRun\":true"));
    assert!(!fixture.path(".aws/config").exists());
    assert_eq!(
        fs::read_to_string(fixture.path(".aws/current-profile")).unwrap(),
        "local-selection"
    );
}
