use std::fs;
use std::time::Instant;

use crate::support::{ACCOUNT, DOCUMENT, Fixture, VAULT, success};

#[test]
fn sync_uses_explicit_human_account_references_and_strips_other_credential_context() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    let catalog = fixture.catalog();
    fixture.op(&format!(
        "printf '%s\\n' \"$@\" > '{}'\nenv > '{}'\ncat '{}'",
        fixture.path("args").display(),
        fixture.path("env").display(),
        catalog.display()
    ));
    let output = fixture
        .command()
        .env("OP_SERVICE_ACCOUNT_TOKEN", "fictional-op-canary")
        .env("OP_CONNECT_TOKEN", "fictional-connect-canary")
        .env("OP_BIOMETRIC_UNLOCK_ENABLED", "false")
        .env("AWS_SECRET_ACCESS_KEY", "fictional-aws-canary")
        .env("PULUMI_ACCESS_TOKEN", "fictional-pulumi-canary")
        .env("GH_TOKEN", "fictional-github-canary")
        .args(["config", "sync"])
        .output()
        .unwrap();
    let stdout = success(output);
    assert!(!stdout.contains("111111111111"));
    assert!(!stdout.contains("example.invalid"));
    let args = fs::read_to_string(fixture.path("args")).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        [
            "--account",
            ACCOUNT,
            "--no-color",
            "document",
            "get",
            DOCUMENT,
            "--vault",
            VAULT
        ]
    );
    let environment = fs::read_to_string(fixture.path("env")).unwrap();
    assert!(!environment.contains("canary"));
    assert!(!environment.contains("OP_SERVICE_ACCOUNT_TOKEN"));
    assert!(environment.contains("OP_BIOMETRIC_UNLOCK_ENABLED=true"));
    assert!(
        fs::read_to_string(fixture.path(".aws/config"))
            .unwrap()
            .contains("sso_session = fixture-login")
    );
}

#[test]
fn canonical_account_identifier_case_is_preserved_for_native_op_lookup() {
    let fixture = Fixture::new();
    let account = ACCOUNT.to_ascii_uppercase();
    success(fixture.run(&[
        "config",
        "source",
        "add",
        "fixture-source",
        "--account",
        &account,
        "--vault",
        VAULT,
        "--document",
        DOCUMENT,
    ]));
    let catalog = fixture.catalog();
    fixture.op(&format!(
        "printf '%s' \"$2\" > '{}'\ncat '{}'",
        fixture.path("account-argument").display(),
        catalog.display()
    ));
    success(fixture.run(&["config", "sync"]));
    assert_eq!(
        fs::read_to_string(fixture.path("account-argument")).unwrap(),
        account
    );
}

#[test]
fn a_failed_or_invalid_document_never_changes_last_good_configuration_or_leaks_diagnostics() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    let catalog = fixture.catalog();
    fixture.op(&format!("cat '{}'", catalog.display()));
    success(fixture.run(&["config", "sync"]));
    let before = fs::read(fixture.path(".aws/config")).unwrap();
    for script in [
        "printf 'fictional-sensitive-canary' >&2; exit 7",
        "printf '[profile fixture]\\naws_secret_access_key=fictional-sensitive-canary\\n'",
        "printf '\\377\\376'",
    ] {
        fixture.op(script);
        let output = fixture.run(&["config", "sync"]);
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("fictional-sensitive-canary"));
        assert_eq!(fs::read(fixture.path(".aws/config")).unwrap(), before);
    }
}

#[test]
fn multiple_sources_publish_together_and_missing_sources_never_partially_install() {
    let fixture = Fixture::new();
    fixture.bind("first", DOCUMENT);
    fixture.bind("second", "dddddddddddddddddddddddddd");
    let catalog = fixture.catalog();
    fixture.op(&format!(
        "case \"$6\" in\n {DOCUMENT}) cat '{}' ;;\n *) printf 'fictional-private-diagnostic' >&2; exit 8 ;;\nesac",
        catalog.display()
    ));
    let output = fixture.run(&["config", "sync"]);
    assert!(!output.status.success());
    assert!(!fixture.path(".aws").exists());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("fictional-private-diagnostic"));
}

#[test]
fn timeouts_are_wall_clock_bounded_and_keep_native_configuration_unchanged() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    fixture.write(".aws/config", "[profile local]\nregion=us-east-1\n");
    fixture.op("sleep 20");
    let started = Instant::now();
    let output = fixture.run(&["config", "sync", "--timeout", "1"]);
    assert!(!output.status.success());
    assert!(started.elapsed().as_secs() < 5);
    assert_eq!(
        fs::read_to_string(fixture.path(".aws/config")).unwrap(),
        "[profile local]\nregion=us-east-1\n"
    );
}

#[test]
fn configuration_edits_during_document_retrieval_are_not_overwritten() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    fixture.write(".aws/config", "[profile local]\nregion=us-east-1\n");
    let catalog = fixture.catalog();
    fixture.op(&format!(
        "printf '[profile concurrent]\\nregion=us-west-2\\n' > '{}'\ncat '{}'",
        fixture.path(".aws/config").display(),
        catalog.display()
    ));
    assert!(!fixture.run(&["config", "sync"]).status.success());
    assert_eq!(
        fs::read_to_string(fixture.path(".aws/config")).unwrap(),
        "[profile concurrent]\nregion=us-west-2\n"
    );
}
