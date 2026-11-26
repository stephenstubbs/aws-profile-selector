use aws_profile_selector::metadata::catalog::Catalog;

fn catalog() -> String {
    "[profile fixture]\nsso_session=fixture-login\nsso_account_id=111111111111\n\
     sso_role_name=Developer\nregion=eu-west-1\n\
     [sso-session fixture-login]\nsso_start_url=https://example.invalid/start\n\
     sso_region=eu-west-1\nsso_registration_scopes=sso:account:access\n"
        .to_owned()
}

#[test]
fn canonical_metadata_round_trips_without_changing_identity() {
    let parsed = Catalog::parse(&catalog()).unwrap();
    assert_eq!(parsed.profile_count(), 1);
    assert_eq!(parsed.session_count(), 1);
    assert_eq!(
        Catalog::parse(&parsed.render()).unwrap().sections,
        parsed.sections
    );
    assert_eq!(
        parsed.effective_profile("profile fixture").unwrap()["sso_account_id"],
        "111111111111"
    );
}

#[test]
fn rejects_credentials_executables_and_unknown_settings_without_echoing_values() {
    for key in [
        "aws_access_key_id",
        "aws_secret_access_key",
        "aws_session_token",
        "credential_process",
        "cli_pager",
        "endpoint_url",
        "unknown",
    ] {
        let text = catalog().replace(
            "region=eu-west-1",
            &format!("{key}=fictional-sensitive-canary"),
        );
        let error = Catalog::parse(&text).unwrap_err().to_string();
        assert!(!error.contains("fictional-sensitive-canary"));
        assert!(!error.contains("111111111111"));
    }
}

#[test]
fn rejects_unresolved_sessions_duplicate_keys_and_foreign_sections() {
    for text in [
        catalog().replace("sso_session=fixture-login", "sso_session=undefined"),
        catalog().replace(
            "sso_role_name=Developer",
            "sso_role_name=Developer\nsso_role_name=Admin",
        ),
        catalog() + "[default]\nregion=us-east-1\n",
        catalog() + "[profile fixture]\nsso_session=fixture-login\n",
        catalog().replace("region=eu-west-1", "region=eu-west-1\n  nested=value"),
    ] {
        assert!(Catalog::parse(&text).is_err());
    }
}

#[test]
fn validates_urls_accounts_regions_roles_and_registration_scopes() {
    for (from, to) in [
        (
            "https://example.invalid/start",
            "https://user:canary@example.invalid/start",
        ),
        (
            "https://example.invalid/start",
            "https://example.invalid/start?token=canary",
        ),
        ("https://example.invalid/start", "file:///tmp/profile"),
        (
            "https://example.invalid/start",
            "https://example.invalid:not-a-port/start",
        ),
        (
            "https://example.invalid/start",
            "https://example..invalid/start",
        ),
        ("111111111111", "111"),
        ("Developer", "role\ninjected=value"),
        ("eu-west-1", "not_a_region"),
        ("sso:account:access", "different-scope"),
    ] {
        assert!(Catalog::parse(&catalog().replace(from, to)).is_err());
    }
}

#[test]
fn merging_never_resolves_identity_by_url_or_silently_overwrites_a_source() {
    let mut first = Catalog::parse(&catalog()).unwrap();
    assert!(first.merge(Catalog::parse(&catalog()).unwrap()).is_err());
    let second = catalog()
        .replace("fixture-login", "second-login")
        .replace("profile fixture", "profile second")
        .replace("111111111111", "222222222222");
    first.merge(Catalog::parse(&second).unwrap()).unwrap();
    assert_eq!(first.profile_count(), 2);
    assert_eq!(first.session_count(), 2);
    first.validate().unwrap();
}
