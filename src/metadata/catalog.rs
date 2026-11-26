use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail, ensure};

use super::ini;

pub type Values = BTreeMap<String, String>;

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub sections: BTreeMap<String, Values>,
}

const PROFILE_KEYS: [&str; 5] = [
    "sso_session",
    "sso_account_id",
    "sso_role_name",
    "region",
    "output",
];
const SESSION_KEYS: [&str; 3] = ["sso_start_url", "sso_region", "sso_registration_scopes"];

impl Catalog {
    pub fn parse(text: &str) -> Result<Self> {
        let mut catalog = Self::default();
        for section in ini::parse(text)? {
            ensure!(
                !section.complex,
                "catalog contains duplicate or unsupported settings"
            );
            let Some(name) = section.name else {
                ensure!(section.values.is_empty(), "catalog settings need a section");
                continue;
            };
            ensure!(
                catalog.sections.insert(name, section.values).is_none(),
                "catalog contains duplicate sections"
            );
        }
        catalog.validate()?;
        Ok(catalog)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(!self.sections.is_empty(), "SSO catalog is empty");
        for (name, values) in &self.sections {
            if let Some(profile) = name.strip_prefix("profile ") {
                ensure!(
                    identifier(profile) && profile != "default",
                    "invalid SSO profile name"
                );
                allowed(values, &PROFILE_KEYS)?;
                let session = required(values, "sso_session")?;
                ensure!(identifier(session), "invalid SSO session reference");
                ensure!(
                    self.sections
                        .contains_key(&format!("sso-session {session}")),
                    "profile references an undefined SSO session"
                );
                let account = required(values, "sso_account_id")?;
                ensure!(
                    account.len() == 12 && account.bytes().all(|byte| byte.is_ascii_digit()),
                    "invalid AWS account identifier"
                );
                let role = required(values, "sso_role_name")?;
                ensure!(
                    role.len() <= 128
                        && role.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || b"_+=,.@-".contains(&byte)
                        }),
                    "invalid SSO role name"
                );
                if let Some(region) = values.get("region") {
                    ensure!(valid_region(region), "invalid API region");
                }
                if let Some(output) = values.get("output") {
                    ensure!(
                        ["json", "yaml", "yaml-stream", "text", "table"].contains(&output.as_str()),
                        "invalid CLI output format"
                    );
                }
            } else if let Some(session) = name.strip_prefix("sso-session ") {
                ensure!(identifier(session), "invalid SSO session name");
                allowed(values, &SESSION_KEYS)?;
                ensure!(
                    valid_start_url(required(values, "sso_start_url")?),
                    "invalid SSO start or issuer URL"
                );
                ensure!(
                    valid_region(required(values, "sso_region")?),
                    "invalid SSO region"
                );
                if let Some(scope) = values.get("sso_registration_scopes") {
                    ensure!(
                        scope == "sso:account:access",
                        "unsupported SSO registration scope"
                    );
                }
            } else {
                bail!("catalog contains a section outside the SSO metadata allowlist");
            }
        }
        ensure!(self.profile_count() > 0, "SSO catalog contains no profiles");
        let used: BTreeSet<_> = self
            .sections
            .values()
            .filter_map(|values| values.get("sso_session"))
            .map(|name| format!("sso-session {name}"))
            .collect();
        ensure!(
            self.sections
                .keys()
                .filter(|name| name.starts_with("sso-session "))
                .all(|name| used.contains(name)),
            "catalog contains an unused SSO session"
        );
        Ok(())
    }

    pub fn merge(&mut self, other: Self) -> Result<()> {
        ensure!(
            other
                .sections
                .keys()
                .all(|name| !self.sections.contains_key(name)),
            "configured sources contain conflicting profile or session names"
        );
        self.sections.extend(other.sections);
        Ok(())
    }

    pub fn profile_count(&self) -> usize {
        self.sections
            .keys()
            .filter(|name| name.starts_with("profile "))
            .count()
    }

    pub fn session_count(&self) -> usize {
        self.sections.len() - self.profile_count()
    }

    pub fn render(&self) -> String {
        let mut output = String::new();
        for (name, values) in &self.sections {
            output.push_str(&format!("[{name}]\n"));
            for (key, value) in values {
                output.push_str(&format!("{key} = {value}\n"));
            }
            output.push('\n');
        }
        output
    }

    pub fn effective_profile(&self, name: &str) -> Option<Values> {
        let mut values = self.sections.get(name)?.clone();
        let session = values.remove("sso_session")?;
        values.extend(
            self.sections
                .get(&format!("sso-session {session}"))?
                .clone(),
        );
        values
            .entry("sso_registration_scopes".to_owned())
            .or_insert_with(|| "sso:account:access".to_owned());
        Some(values)
    }
}

pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_.@-".contains(&byte))
}

fn allowed(values: &Values, keys: &[&str]) -> Result<()> {
    ensure!(
        values.keys().all(|key| keys.contains(&key.as_str())),
        "catalog contains a setting outside the SSO metadata allowlist"
    );
    ensure!(
        values
            .values()
            .all(|value| !value.is_empty() && !value.chars().any(char::is_control)),
        "catalog contains an empty or invalid value"
    );
    Ok(())
}

fn required<'a>(values: &'a Values, key: &str) -> Result<&'a str> {
    values
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| anyhow::anyhow!("catalog is missing a required SSO setting"))
}

fn valid_region(value: &str) -> bool {
    let parts: Vec<_> = value.split('-').collect();
    parts.len() >= 3
        && parts
            .last()
            .is_some_and(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        && parts[..parts.len() - 1].iter().all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}

fn valid_start_url(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("https://") else {
        return false;
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let host = if let Some((host, port)) = authority.split_once(':') {
        if port.parse::<u16>().ok().is_none_or(|value| value == 0) {
            return false;
        }
        host
    } else {
        authority
    };
    value.len() <= 2048
        && value.is_ascii()
        && host.trim_end_matches('.').split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .as_bytes()
                    .last()
                    .is_some_and(u8::is_ascii_alphanumeric)
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
        && !value
            .bytes()
            .any(|b| b.is_ascii_control() || b.is_ascii_whitespace() || b"\\?#".contains(&b))
}
