#[path = "metadata/bounds.rs"]
mod bounds;
#[path = "metadata/fetch.rs"]
mod fetch;
mod support;

use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use support::{DOCUMENT, Fixture, success};

#[test]
fn metadata_status_is_read_only_and_source_bindings_are_private() {
    let fixture = Fixture::new();
    let status = Fixture::json(fixture.run(&["config", "status"]));
    assert_eq!(status["sources"], 0);
    assert!(!fixture.path(".aws").exists());
    assert!(!fixture.path(".config").exists());
    fixture.bind("fixture-source", DOCUMENT);
    let path = fixture.path(".config/aws-profile-selector/sources.json");
    assert_eq!(fs::metadata(&path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(
        fs::metadata(path.parent().unwrap()).unwrap().mode() & 0o777,
        0o700
    );
    assert!(!fixture.path(".aws").exists());
    assert!(
        !fixture
            .run(&[
                "config",
                "source",
                "add",
                "invalid",
                "--account",
                "not-an-id",
                "--vault",
                "not-an-id",
                "--document",
                "not-an-id"
            ])
            .status
            .success()
    );
}

#[test]
fn install_dry_run_and_clear_never_touch_caches_or_saved_selection() {
    let fixture = Fixture::new();
    let catalog = fixture.catalog();
    fixture.write(".aws/current-profile", "fixture");
    fixture.write(".aws/sso/cache/token.json", "fictional-login-canary");
    fixture.write(".aws/cli/cache/role.json", "fictional-role-canary");
    let preview = Fixture::json(fixture.run(&[
        "config",
        "install",
        "--file",
        catalog.to_str().unwrap(),
        "--dry-run",
    ]));
    assert_eq!(preview["profiles"], 1);
    assert!(!fixture.path(".aws/config").exists());
    success(fixture.run(&["config", "install", "--file", catalog.to_str().unwrap()]));
    assert_eq!(
        Fixture::json(fixture.run(&["config", "status"]))["profiles"],
        1
    );
    success(fixture.run(&["config", "clear"]));
    assert_eq!(
        fs::read_to_string(fixture.path(".aws/current-profile")).unwrap(),
        "fixture"
    );
    assert_eq!(
        fs::read_to_string(fixture.path(".aws/sso/cache/token.json")).unwrap(),
        "fictional-login-canary"
    );
    assert_eq!(
        fs::read_to_string(fixture.path(".aws/cli/cache/role.json")).unwrap(),
        "fictional-role-canary"
    );
}

#[test]
fn source_binding_changes_require_an_explicit_replace_and_never_print_references() {
    let fixture = Fixture::new();
    fixture.bind("fixture-source", DOCUMENT);
    let before = fs::read(fixture.path(".config/aws-profile-selector/sources.json")).unwrap();
    let args = [
        "config",
        "source",
        "add",
        "fixture-source",
        "--account",
        support::ACCOUNT,
        "--vault",
        support::VAULT,
        "--document",
        "dddddddddddddddddddddddddd",
    ];
    assert!(!fixture.run(&args).status.success());
    assert_eq!(
        fs::read(fixture.path(".config/aws-profile-selector/sources.json")).unwrap(),
        before
    );
    let mut replace = args.to_vec();
    replace.push("--replace");
    let stdout = success(fixture.run(&replace));
    assert!(!stdout.contains(support::ACCOUNT));
    assert!(!stdout.contains("dddddddddddddddddddddddddd"));
    success(fixture.run(&["config", "source", "remove", "fixture-source"]));
    assert_eq!(
        Fixture::json(fixture.run(&["config", "status"]))["sources"],
        0
    );
}

#[test]
fn explicit_config_overrides_and_workspace_paths_cannot_steer_implicit_writes() {
    let fixture = Fixture::new();
    let catalog = fixture.catalog();
    let output = fixture
        .command()
        .env("AWS_CONFIG_FILE", fixture.path("another-config"))
        .args(["config", "install", "--file", catalog.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!fixture.path(".aws").exists());
    fs::create_dir(fixture.path("project")).unwrap();
    fs::create_dir(fixture.path("project/.git")).unwrap();
    let output = fixture.run(&[
        "config",
        "install",
        "--file",
        catalog.to_str().unwrap(),
        "--target",
        fixture.path("project/config").to_str().unwrap(),
    ]);
    assert!(!output.status.success());
    assert!(!fixture.path("project/config").exists());
}

#[test]
fn unchanged_catalogs_still_repair_private_permissions_without_exposing_data() {
    let fixture = Fixture::new();
    let catalog = fixture.catalog();
    success(fixture.run(&["config", "install", "--file", catalog.to_str().unwrap()]));
    fs::set_permissions(
        fixture.path(".aws/config"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    fs::set_permissions(fixture.path(".aws"), fs::Permissions::from_mode(0o755)).unwrap();
    let report =
        Fixture::json(fixture.run(&["config", "install", "--file", catalog.to_str().unwrap()]));
    assert_eq!(report["changed"], true);
    assert_eq!(
        fs::metadata(fixture.path(".aws/config")).unwrap().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(fixture.path(".aws")).unwrap().mode() & 0o777,
        0o700
    );
}
