use super::*;
use tempfile::tempdir;

fn parse(config: &str, credentials: &str) -> Vec<Profile> {
    parse_contents(Some(config), Some(credentials))
}

fn names(profiles: &[Profile]) -> Vec<&str> {
    profiles.iter().map(|p| p.name.as_str()).collect()
}

fn source(path: PathBuf, label: &'static str, required: bool) -> Source {
    Source {
        path,
        label,
        required,
    }
}

#[test]
fn parses_default_and_named_config_profiles_sorted() {
    let config = r#"[profile prod]
region = us-east-1
sso_account_id = 987654321098
sso_role_name = ReadOnlyAccess
[default]
region = eu-west-1
[profile dev]
sso_account_id = 123456789012
sso_role_name = DeveloperAccess
region = us-west-2
"#;
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["default", "dev", "prod"]);
    assert_eq!(profiles[0].get_region(), Some("eu-west-1"));
    assert_eq!(profiles[1].get_account_id(), Some("123456789012"));
    assert_eq!(profiles[1].get_role_name(), Some("DeveloperAccess"));
    assert_eq!(profiles[1].get_region(), Some("us-west-2"));
    assert_eq!(profiles[2].get_account_id(), Some("987654321098"));
    assert!(profiles.iter().all(|p| {
        p.attributes
            .keys()
            .all(|k| DISPLAY_KEYS.contains(&k.as_str()))
    }));
}

#[test]
fn non_profile_sections_do_not_leak_into_neighbours() {
    let config = r#"[profile alpha]
region = us-east-1
[sso-session corp]
sso_start_url = https://example.awsapps.com/start
region = leaked
[services build]
sso_account_id = 999999999999
region = leaked-again
[dev]
region = bare-section
[profile beta]
region = ap-southeast-2
[plugins]
region = leaked-plugin
"#;
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["alpha", "beta"]);
    assert_eq!(profiles[0].get_region(), Some("us-east-1"));
    assert_eq!(profiles[0].get_account_id(), None);
    assert_eq!(profiles[0].attributes.len(), 1);
    assert_eq!(profiles[1].get_region(), Some("ap-southeast-2"));
    assert_eq!(profiles[1].attributes.len(), 1);
}

#[test]
fn duplicate_profile_sections_merge_and_later_values_win() {
    let config = "[profile dev]\nregion = us-east-1\nsso_role_name = ReadOnly\n[profile dev]\nsso_account_id = 42\nregion = us-west-2\n";
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["dev"]);
    let dev = &profiles[0];
    assert_eq!(dev.get_region(), Some("us-west-2"));
    assert_eq!(dev.get_account_id(), Some("42"));
    assert_eq!(dev.get_role_name(), Some("ReadOnly"));
    assert_eq!(dev.attributes.len(), 3);
}

#[test]
fn accepts_whitespace_comments_and_embedded_equals() {
    let config = "  # full-line comment\n;\tsemicolon comment\n\n   [ profile\t spaced ]   # trailing comment\n   sso_role_name = Team=ReadOnly#frag\n   region=us-east-1\n   ; ignored\n   # ignored\n[profile next] ; also a comment\nregion = eu-west-1\n";
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["next", "spaced"]);
    assert_eq!(profiles[0].get_region(), Some("eu-west-1"));
    assert_eq!(profiles[1].get_region(), Some("us-east-1"));
    assert_eq!(profiles[1].get_role_name(), Some("Team=ReadOnly#frag"));
}

#[test]
fn ignores_invalid_empty_profile_names() {
    let config = "[]\nregion = dropped\n[profile]\nregion = dropped\n[profile   ]\nregion = dropped\n[profile \"\"]\nregion = dropped\n[profile kept]\nregion = us-east-1\n";
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["kept"]);
    assert_eq!(profiles[0].get_region(), Some("us-east-1"));
}

#[test]
fn credentials_supply_identity_and_config_supplies_metadata() {
    let config = "[default]\nregion = us-east-1\naws_secret_access_key = shhh\n[solo]\nregion = ignored-bare\n[profile tooling]\nsso_account_id = 111111111111\naws_session_token = tok\n";
    let credentials = "[default]\naws_access_key_id = AKIAIOSFODNN7EXAMPLE\naws_secret_access_key = wJalrXUtnFEMI/K7MDENG\nregion = us-west-1\n\n[tooling]\naws_access_key_id = AKIAI44QH8DHBEXAMPLE\nregion = eu-central-1\n\n[solo]\naws_access_key_id = AKIA\nregion = sa-east-1\n";
    let profiles = parse(config, credentials);

    assert_eq!(names(&profiles), ["default", "solo", "tooling"]);
    assert_eq!(profiles[0].get_region(), Some("us-east-1"));
    assert!(profiles[1].attributes.is_empty());
    assert_eq!(profiles[2].get_account_id(), Some("111111111111"));
    assert_eq!(profiles[2].get_region(), None);

    let rendered = format!("{profiles:?}");
    for leaked in [
        "ignored-bare",
        "us-west-1",
        "eu-central-1",
        "sa-east-1",
        "AKIAIOSFODNN7EXAMPLE",
        "AKIAI44QH8DHBEXAMPLE",
        "wJalrXUtnFEMI",
        "shhh",
    ] {
        assert!(!rendered.contains(leaked), "leaked: {leaked}");
    }
}

#[test]
fn nested_settings_do_not_replace_top_level_metadata() {
    let config = "[profile dev]\nregion = us-east-1\nsso_account_id = 123456789012\ns3 =\n    region = eu-west-1\n    sso_account_id = 999999999999\n    max_queue_size = 1000\nsso_role_name = ReadOnly\n";
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["dev"]);
    assert_eq!(profiles[0].get_region(), Some("us-east-1"));
    assert_eq!(profiles[0].get_account_id(), Some("123456789012"));
    assert_eq!(profiles[0].get_role_name(), Some("ReadOnly"));
    assert_eq!(profiles[0].attributes.len(), 3);
}

#[test]
fn nested_settings_use_indentation_relative_to_the_parent() {
    let config = "[profile spaced]\n    region = us-west-2\n    s3 =\n      region = eu-central-1\n      max_queue_size = 1000\n    sso_role_name = Developer\n";
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["spaced"]);
    assert_eq!(profiles[0].get_region(), Some("us-west-2"));
    assert_eq!(profiles[0].get_role_name(), Some("Developer"));
    assert_eq!(profiles[0].attributes.len(), 2);
}

#[test]
fn nested_blocks_end_only_when_indentation_dedents_to_the_parent() {
    let config = "[profile deep]\ns3 =\n    foo =\n        region = sa-east-1\n    sso_account_id = 999999999999\n";
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["deep"]);
    assert!(profiles[0].attributes.is_empty());
}

#[test]
fn resolves_quoted_profile_names_and_case_sensitive_sections() {
    let config = "[profile \"team name\"] # trailing comment\nregion = us-east-1\n[profile 'single quoted']\nsso_role_name = Single\n[profile Dev]\nsso_role_name = Admin\n[Profile ignored]\nregion = eu-west-1\n[DEFAULT]\nregion = ap-south-1\n[Default]\nregion = ap-northeast-1\n[profile team name]\nregion = eu-central-1\n[profile \"quoted\" trailing]\nregion = eu-north-1\n";
    let profiles = parse(config, "");

    assert_eq!(names(&profiles), ["Dev", "single quoted", "team name"]);
    assert_eq!(profiles[0].get_role_name(), Some("Admin"));
    assert_eq!(profiles[1].get_role_name(), Some("Single"));
    assert_eq!(profiles[2].get_region(), Some("us-east-1"));
}

#[test]
fn credentials_section_names_are_case_sensitive_and_default_is_special() {
    let profiles = parse(
        "",
        "[Dev]\naws_access_key_id = a\n[dev]\naws_access_key_id = b\n[DEFAULT]\naws_access_key_id = c\n[services]\n[sso-session work]\n",
    );

    assert_eq!(
        names(&profiles),
        ["Dev", "dev", "services", "sso-session work"]
    );
}

#[test]
fn decodes_escaped_and_mixed_quotes_without_accepting_extra_names() {
    let profiles = parse(
        r#"[profile team\ name]
[profile team" prod"]
[profile "a\"b"]
[profile "a\\b"]
[profile dev#team]
[profile "dev" "prod"]
[profile unfinished\]
[profile "unfinished]
"#,
        "",
    );
    assert_eq!(
        names(&profiles),
        ["a\"b", "a\\b", "dev#team", "team name", "team prod"]
    );
}

#[test]
fn inherits_default_metadata_but_keeps_explicit_empty_values_and_profile_identity() {
    let profiles = parse(
        "[DEFAULT]\nregion = us-east-1\nsso_role_name = Reader\n\
         [profile inherited]\n[profile overridden]\nregion = eu-west-1\n\
         [profile empty]\nregion =\n[profile DEFAULT]\n",
        "[credentials-only]\n",
    );
    assert_eq!(
        names(&profiles),
        [
            "DEFAULT",
            "credentials-only",
            "empty",
            "inherited",
            "overridden"
        ]
    );
    assert_eq!(profiles[0].get_region(), Some("us-east-1"));
    assert!(profiles[1].attributes.is_empty());
    assert_eq!(profiles[2].get_region(), None);
    assert_eq!(profiles[2].get_role_name(), Some("Reader"));
    assert_eq!(profiles[3].get_region(), Some("us-east-1"));
    assert_eq!(profiles[4].get_region(), Some("eu-west-1"));
}

#[test]
fn later_empty_metadata_clears_previous_values() {
    let profiles = parse(
        "[profile dev]\nregion = us-east-1\n[profile dev]\nregion =\n",
        "",
    );
    assert!(profiles[0].attributes.is_empty());
}

#[test]
fn missing_default_sources_are_not_errors() {
    let dir = tempdir().expect("tempdir");
    let result = load_profiles(
        &source(dir.path().join("config"), "config", false),
        &source(dir.path().join("credentials"), "credentials", false),
    );

    assert!(result.expect("missing defaults are fine").is_empty());
}

#[test]
fn explicitly_configured_missing_source_is_a_contextual_error() {
    let dir = tempdir().expect("tempdir");
    let result = load_profiles(
        &source(dir.path().join("custom-config"), "config", true),
        &source(dir.path().join("credentials"), "credentials", false),
    );

    let error = result.expect_err("explicit file must exist").to_string();
    assert!(error.contains("failed to read AWS config file"), "{error}");
    assert!(error.contains("custom-config"), "{error}");
}

#[test]
#[cfg(unix)]
fn unreadable_source_is_a_contextual_error() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().expect("tempdir");
    let path = dir.path().join("config");
    fs::write(&path, "[default]\nregion = us-east-1\n").expect("write config");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod");
    let result = load_profiles(
        &source(path.clone(), "config", true),
        &source(dir.path().join("credentials"), "credentials", false),
    );
    if fs::read_to_string(&path).is_ok() {
        return; // running as root: permission bits do not apply
    }

    let error = result.expect_err("unreadable file must error").to_string();
    assert!(error.contains("failed to read AWS config file"), "{error}");
    assert!(!error.contains("us-east-1"), "contents leaked: {error}");
}
