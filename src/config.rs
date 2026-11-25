use anyhow::{Context, Result};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

const CONFIG_ENV: &str = "AWS_CONFIG_FILE";
const CREDENTIALS_ENV: &str = "AWS_SHARED_CREDENTIALS_FILE";

/// The only config values the selector displays.
const DISPLAY_KEYS: [&str; 3] = ["sso_account_id", "region", "sso_role_name"];

#[derive(Debug)]
pub struct Profile {
    pub name: String,
    pub attributes: HashMap<String, String>,
}

impl Profile {
    pub fn get_account_id(&self) -> Option<&str> {
        self.attributes.get("sso_account_id").map(String::as_str)
    }

    pub fn get_region(&self) -> Option<&str> {
        self.attributes.get("region").map(String::as_str)
    }

    pub fn get_role_name(&self) -> Option<&str> {
        self.attributes.get("sso_role_name").map(String::as_str)
    }
}

#[derive(Clone, Copy)]
enum FileKind {
    Config,
    Credentials,
}

struct Source {
    path: PathBuf,
    label: &'static str,
    required: bool,
}

pub fn read_aws_config() -> Result<Vec<Profile>> {
    let home = env::home_dir();
    let default_source = |label: &'static str, file: &'static str| -> Result<Source> {
        let home = home.as_ref().ok_or_else(|| {
            anyhow::anyhow!("unable to determine home directory for ~/.aws/{file}")
        })?;
        Ok(Source {
            path: home.join(".aws").join(file),
            label,
            required: false,
        })
    };

    let config = match override_path(CONFIG_ENV) {
        Some(path) => Source {
            path,
            label: "config",
            required: true,
        },
        None => default_source("config", "config")?,
    };
    let credentials = match override_path(CREDENTIALS_ENV) {
        Some(path) => Source {
            path,
            label: "credentials",
            required: true,
        },
        None => default_source("credentials", "credentials")?,
    };

    load_profiles(&config, &credentials)
}

fn override_path(variable: &str) -> Option<PathBuf> {
    env::var_os(variable)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn load_profiles(config: &Source, credentials: &Source) -> Result<Vec<Profile>> {
    let credentials = read_source(credentials)?;
    let config = read_source(config)?;
    Ok(parse_contents(config.as_deref(), credentials.as_deref()))
}

fn read_source(source: &Source) -> Result<Option<String>> {
    match fs::read_to_string(&source.path) {
        Ok(content) => Ok(Some(content)),
        // Absent default files are normal; an explicitly configured file must exist.
        Err(error) if error.kind() == ErrorKind::NotFound && !source.required => Ok(None),
        Err(error) => Err(error).with_context(|| {
            format!(
                "failed to read AWS {} file {}",
                source.label,
                source.path.display()
            )
        }),
    }
}

fn parse_contents(config: Option<&str>, credentials: Option<&str>) -> Vec<Profile> {
    let mut profiles: BTreeMap<String, HashMap<String, String>> = BTreeMap::new();
    // Credentials first so config values win; BTreeMap keeps names sorted.
    if let Some(content) = credentials {
        collect_profiles(content, FileKind::Credentials, &mut profiles);
    }
    if let Some(content) = config {
        collect_profiles(content, FileKind::Config, &mut profiles);
    }
    profiles
        .into_iter()
        .map(|(name, mut attributes)| {
            attributes.retain(|_, value| !value.is_empty());
            Profile { name, attributes }
        })
        .collect()
}

fn collect_profiles(
    content: &str,
    kind: FileKind,
    profiles: &mut BTreeMap<String, HashMap<String, String>>,
) {
    let mut parsed = BTreeMap::new();
    let mut defaults = HashMap::new();
    let mut in_defaults = false;
    let mut active: Option<(String, HashMap<String, String>)> = None;
    let mut key_indent: Option<usize> = None;

    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        // Any `[...]` line is a section boundary, even when malformed.
        if line.starts_with('[') {
            flush(&mut active, &mut parsed);
            let section = section_body(line);
            in_defaults = matches!(kind, FileKind::Config) && section == Some("DEFAULT");
            active = section
                .and_then(|body| profile_name(body, kind))
                .map(|name| (name, HashMap::new()));
            key_indent = None;
            continue;
        }

        let attributes = if in_defaults {
            &mut defaults
        } else if let Some((_, attributes)) = active.as_mut() {
            attributes
        } else {
            continue;
        };

        // AWS nests settings under the preceding key by indentation alone.
        let indent = raw.len() - raw.trim_start().len();
        if key_indent.is_some_and(|parent| indent > parent) {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        key_indent = Some(indent);

        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if !matches!(kind, FileKind::Config) || !DISPLAY_KEYS.contains(&key.as_str()) {
            continue;
        }
        attributes.insert(key, value.to_string());
    }

    flush(&mut active, &mut parsed);
    for (name, mut attributes) in parsed {
        for (key, value) in &defaults {
            attributes
                .entry(key.clone())
                .or_insert_with(|| value.clone());
        }
        profiles.entry(name).or_default().extend(attributes);
    }
}

fn flush(
    active: &mut Option<(String, HashMap<String, String>)>,
    profiles: &mut BTreeMap<String, HashMap<String, String>>,
) {
    if let Some((name, attributes)) = active.take() {
        profiles.entry(name).or_default().extend(attributes);
    }
}

/// Returns the text inside `[...]`, allowing a trailing `#`/`;` comment.
fn section_body(line: &str) -> Option<&str> {
    let (body, after) = line.strip_prefix('[')?.split_once(']')?;
    let after = after.trim();
    if after.is_empty() || after.starts_with('#') || after.starts_with(';') {
        Some(body.trim())
    } else {
        None
    }
}

fn profile_name(section: &str, kind: FileKind) -> Option<String> {
    if section.is_empty() {
        return None;
    }

    match kind {
        FileKind::Config => {
            if section == "default" {
                return Some("default".to_string());
            }
            let (head, rest) = split_head(section)?;
            if head != "profile" {
                return None;
            }
            decode_profile_name(rest.trim())
        }
        // Credentials sections are literal names; DEFAULT is configparser's defaults section.
        FileKind::Credentials => {
            if section == "DEFAULT" {
                None
            } else {
                Some(section.to_string())
            }
        }
    }
}

fn split_head(section: &str) -> Option<(&str, &str)> {
    section
        .find(char::is_whitespace)
        .map(|index| (&section[..index], &section[index..]))
}

fn decode_profile_name(value: &str) -> Option<String> {
    let mut name = String::new();
    let mut quote = None;
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        match (quote, ch) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('\''), ch) => name.push(ch),
            (_, '\\') => {
                let next = chars.next()?;
                if quote == Some('"') && !matches!(next, '"' | '\\') {
                    name.push('\\');
                }
                name.push(next);
            }
            (None, '\'' | '"') => quote = Some(ch),
            (None, ch) if ch.is_whitespace() => return None,
            (_, ch) => name.push(ch),
        }
    }
    (quote.is_none() && !name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests;
