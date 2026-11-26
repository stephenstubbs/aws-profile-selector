use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, ensure};

use super::catalog::{Catalog, Values};
use super::ini::{self, Section};

pub const BEGIN: &str = "# >>> aws-profile-selector managed SSO metadata v1 >>>\n";
pub const END: &str = "# <<< aws-profile-selector managed SSO metadata v1 <<<\n";

pub fn split_managed(text: &str) -> Result<(String, Option<Catalog>)> {
    let begin_count = text.matches(BEGIN).count();
    let end_count = text.matches(END).count();
    ensure!(
        begin_count == end_count && begin_count <= 1,
        "managed metadata boundaries are invalid"
    );
    let Some(begin) = text.find(BEGIN) else {
        ensure!(
            !text.contains("aws-profile-selector managed SSO metadata"),
            "managed metadata boundary was modified"
        );
        return Ok((text.to_owned(), None));
    };
    let end = text
        .find(END)
        .ok_or_else(|| anyhow::anyhow!("missing metadata boundary"))?;
    ensure!(
        (begin == 0 || text[..begin].ends_with('\n')) && (end == 0 || text[..end].ends_with('\n')),
        "managed metadata boundaries must occupy complete lines"
    );
    ensure!(
        end >= begin + BEGIN.len(),
        "managed metadata boundaries are out of order"
    );
    let previous = Catalog::parse(&text[begin + BEGIN.len()..end])?;
    let outside = format!("{}{}", &text[..begin], &text[end + END.len()..]);
    Ok((outside, Some(previous)))
}

pub fn merge(existing: &str, incoming: &Catalog, adopt: bool) -> Result<String> {
    merge_internal(existing, incoming, adopt, false)
}

pub fn merge_with_identity_approval(
    existing: &str,
    incoming: &Catalog,
    adopt: bool,
) -> Result<String> {
    merge_internal(existing, incoming, adopt, true)
}

fn merge_internal(
    existing: &str,
    incoming: &Catalog,
    adopt: bool,
    approve_identity: bool,
) -> Result<String> {
    incoming.validate()?;
    let (outside, previous) = split_managed(existing)?;
    if let Some(previous) = &previous {
        for name in previous
            .sections
            .keys()
            .filter(|name| name.starts_with("profile "))
        {
            if incoming.sections.contains_key(name) {
                ensure!(
                    approve_identity || identity(previous, name) == identity(incoming, name),
                    "managed profile identity changed; review the catalog and use --accept-identity-change explicitly"
                );
            }
        }
    }
    let chunks = ini::parse(&outside)?;
    let mut existing_sections = BTreeMap::new();
    for chunk in &chunks {
        if let Some(name) = &chunk.name {
            ensure!(
                existing_sections.insert(name.clone(), chunk).is_none(),
                "local configuration has duplicate sections; review it before syncing"
            );
        }
    }
    let mut adopted = BTreeSet::new();
    for (name, values) in &incoming.sections {
        if let Some(section) = existing_sections.get(name) {
            ensure!(
                adopt,
                "an unmanaged section conflicts; review and use --adopt to import matching SSO metadata"
            );
            ensure!(
                equivalent(section, name, values, incoming, &existing_sections)?,
                "existing SSO metadata differs or includes other settings; no configuration was replaced"
            );
            adopted.insert(name.clone());
        }
    }
    protect_dependencies(&chunks, previous.as_ref(), Some(incoming), &adopted)?;
    let mut output = String::new();
    for chunk in chunks {
        if chunk
            .name
            .as_ref()
            .is_none_or(|name| !adopted.contains(name))
        {
            output.push_str(&chunk.raw);
        }
    }
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    if !output.is_empty() && !output.ends_with("\n\n") {
        output.push('\n');
    }
    output.push_str(BEGIN);
    output.push_str(&incoming.render());
    output.push_str(END);
    Ok(output)
}

pub fn clear(existing: &str) -> Result<String> {
    let (outside, previous) = split_managed(existing)?;
    protect_dependencies(
        &ini::parse(&outside)?,
        previous.as_ref(),
        None,
        &BTreeSet::new(),
    )?;
    Ok(outside)
}

fn equivalent(
    section: &Section,
    name: &str,
    incoming: &Values,
    catalog: &Catalog,
    local: &BTreeMap<String, &Section>,
) -> Result<bool> {
    if section.complex {
        return Ok(false);
    }
    if name.starts_with("sso-session ") {
        let mut old = section.values.clone();
        let mut new = incoming.clone();
        old.entry("sso_registration_scopes".to_owned())
            .or_insert_with(|| "sso:account:access".to_owned());
        new.entry("sso_registration_scopes".to_owned())
            .or_insert_with(|| "sso:account:access".to_owned());
        return Ok(old == new);
    }
    let allowed = [
        "sso_session",
        "sso_start_url",
        "sso_region",
        "sso_registration_scopes",
        "sso_account_id",
        "sso_role_name",
        "region",
        "output",
    ];
    if section
        .values
        .keys()
        .any(|key| !allowed.contains(&key.as_str()))
    {
        return Ok(false);
    }
    let mut old = section.values.clone();
    if let Some(session) = old.remove("sso_session") {
        if incoming.get("sso_session") != Some(&session) {
            return Ok(false);
        }
        if let Some(definition) = local.get(&format!("sso-session {session}")) {
            if definition.complex {
                return Ok(false);
            }
            for (key, value) in &definition.values {
                if old.get(key).is_some_and(|existing| existing != value) {
                    return Ok(false);
                }
                old.insert(key.clone(), value.clone());
            }
        } else if !old.contains_key("sso_start_url") || !old.contains_key("sso_region") {
            return Ok(false);
        }
    }
    old.entry("sso_registration_scopes".to_owned())
        .or_insert_with(|| "sso:account:access".to_owned());
    Ok(catalog
        .effective_profile(name)
        .is_some_and(|expected| expected == old))
}

fn identity(catalog: &Catalog, name: &str) -> Option<Values> {
    let mut values = catalog.effective_profile(name)?;
    values.retain(|key, _| {
        [
            "sso_start_url",
            "sso_region",
            "sso_account_id",
            "sso_role_name",
        ]
        .contains(&key.as_str())
    });
    values.insert(
        "sso_session".to_owned(),
        catalog.sections.get(name)?.get("sso_session")?.clone(),
    );
    Some(values)
}

fn protect_dependencies(
    outside: &[Section],
    previous: Option<&Catalog>,
    incoming: Option<&Catalog>,
    adopted: &BTreeSet<String>,
) -> Result<()> {
    let Some(previous) = previous else {
        return Ok(());
    };
    for section in outside {
        if section
            .name
            .as_ref()
            .is_some_and(|name| adopted.contains(name))
        {
            continue;
        }
        for (key, prefix) in [
            ("sso_session", "sso-session "),
            ("source_profile", "profile "),
        ] {
            if let Some(reference) = section.values.get(key) {
                let name = format!("{prefix}{reference}");
                if let Some(old) = previous.sections.get(&name) {
                    ensure!(
                        incoming.and_then(|catalog| catalog.sections.get(&name)) == Some(old),
                        "unmanaged profiles depend on metadata being changed or removed; review those profiles first"
                    );
                }
            }
        }
    }
    Ok(())
}
