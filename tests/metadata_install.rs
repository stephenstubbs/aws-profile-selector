use aws_profile_selector::metadata::catalog::Catalog;
use aws_profile_selector::metadata::install::{self, BEGIN, END};

fn catalog() -> Catalog {
    Catalog::parse(
        "[profile fixture]\nsso_session=fixture-login\nsso_account_id=111111111111\n\
         sso_role_name=Developer\nregion=eu-west-1\n\
         [sso-session fixture-login]\nsso_start_url=https://example.invalid/start\n\
         sso_region=eu-west-1\n",
    )
    .unwrap()
}

#[test]
fn preserves_unmanaged_configuration_byte_for_byte_and_is_idempotent() {
    let unmanaged = "# local preference\n[default]\noutput = json\n\
                     [services local]\ns3 =\n  endpoint_url = http://localhost:4566/\n";
    let first = install::merge(unmanaged, &catalog(), false).unwrap();
    assert!(first.starts_with(unmanaged));
    assert_eq!(first.matches(BEGIN).count(), 1);
    assert_eq!(first.matches(END).count(), 1);
    assert_eq!(install::merge(&first, &catalog(), false).unwrap(), first);
    assert!(install::clear(&first).unwrap().starts_with(unmanaged));
}

#[test]
fn adoption_is_explicit_and_only_accepts_matching_metadata() {
    let old = "[profile fixture]\nsso_start_url=https://example.invalid/start\n\
               sso_region=eu-west-1\nsso_account_id=111111111111\n\
               sso_role_name=Developer\nregion=eu-west-1\n";
    assert!(install::merge(old, &catalog(), false).is_err());
    let adopted = install::merge(old, &catalog(), true).unwrap();
    assert_eq!(adopted.matches("[profile fixture]").count(), 1);
    assert!(install::merge(&old.replace("Developer", "DifferentRole"), &catalog(), true).is_err());
    assert!(
        install::merge(
            &(old.to_owned() + "credential_process=do-not-run\n"),
            &catalog(),
            true
        )
        .is_err()
    );
}

#[test]
fn repairs_an_explicit_session_reference_only_when_inline_identity_matches() {
    let old = "[profile fixture]\nsso_session=fixture-login\n\
               sso_start_url=https://example.invalid/start\nsso_region=eu-west-1\n\
               sso_account_id=111111111111\nsso_role_name=Developer\nregion=eu-west-1\n";
    let new = install::merge(old, &catalog(), true).unwrap();
    assert!(new.contains("[sso-session fixture-login]"));
    assert!(
        install::merge(
            &old.replace("111111111111", "222222222222"),
            &catalog(),
            true
        )
        .is_err()
    );
}

#[test]
fn refuses_malformed_owned_markers_and_untracked_authentication_settings() {
    let managed = install::merge("", &catalog(), false).unwrap();
    assert!(install::merge(&managed.replace(END, ""), &catalog(), false).is_err());
    assert!(install::merge(&(managed.clone() + &managed), &catalog(), false).is_err());
    assert!(install::clear(&format!("setting=value{managed}")).is_err());
    let poisoned = managed.replace(
        "sso_role_name = Developer",
        "credential_process = do-not-run",
    );
    assert!(install::merge(&poisoned, &catalog(), false).is_err());
}

#[test]
fn unmanaged_session_and_role_dependencies_prevent_destructive_retirement() {
    let managed = install::merge("", &catalog(), false).unwrap();
    for dependent in [
        "[profile local]\nsso_session=fixture-login\n",
        "[profile local]\nsource_profile=fixture\n",
    ] {
        assert!(install::clear(&(dependent.to_owned() + &managed)).is_err());
    }
    let unrelated = "[profile local]\nregion=us-east-1\n";
    assert!(
        install::clear(&(unrelated.to_owned() + &managed))
            .unwrap()
            .contains(unrelated)
    );
}

#[test]
fn rebinding_a_managed_principal_requires_separate_explicit_approval() {
    let managed = install::merge("", &catalog(), false).unwrap();
    let different =
        Catalog::parse(&catalog().render().replace("111111111111", "222222222222")).unwrap();
    assert!(install::merge(&managed, &different, false).is_err());
    assert!(install::merge(&managed, &different, true).is_err());
    let approved = install::merge_with_identity_approval(&managed, &different, false).unwrap();
    assert!(approved.contains("222222222222"));
    let session_changed = catalog()
        .render()
        .replace("fixture-login", "different-login");
    let different_session = Catalog::parse(&session_changed).unwrap();
    assert!(install::merge(&managed, &different_session, false).is_err());
}

#[test]
fn adoption_does_not_collapse_explicit_login_session_names() {
    let old = catalog()
        .render()
        .replace("fixture-login", "separate-login");
    assert!(install::merge(&old, &catalog(), true).is_err());
}
