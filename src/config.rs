use anyhow::{Context, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
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

pub fn read_aws_config() -> Result<Vec<Profile>> {
    let config_path = get_aws_config_path()?;

    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "AWS config file not found at {:?}",
            config_path
        ));
    }

    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read AWS config file: {config_path:?}"))?;

    parse_aws_config(&content)
}

fn get_aws_config_path() -> Result<PathBuf> {
    let home_dir =
        dirs::home_dir().ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?;

    Ok(home_dir.join(".aws").join("config"))
}

fn parse_aws_config(content: &str) -> Result<Vec<Profile>> {
    let section_regex = Regex::new(r"^\s*\[profile\s+([^\]]+)\]")?;
    let key_value_regex = Regex::new(r"^\s*([^=]+?)\s*=\s*(.*?)\s*$")?;

    let mut profiles = Vec::new();
    let mut current_profile: Option<String> = None;
    let mut current_attributes = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(captures) = section_regex.captures(line) {
            if let Some(profile_name) = current_profile.take() {
                profiles.push(Profile {
                    name: profile_name,
                    attributes: current_attributes.clone(),
                });
                current_attributes.clear();
            }

            current_profile = Some(captures[1].trim().to_string());
            continue;
        }

        if current_profile.is_some() {
            if let Some(captures) = key_value_regex.captures(line) {
                let key = captures[1].trim().to_string();
                let value = captures[2].trim().to_string();
                current_attributes.insert(key, value);
            }
        }
    }

    if let Some(profile_name) = current_profile {
        profiles.push(Profile {
            name: profile_name,
            attributes: current_attributes,
        });
    }

    profiles.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_aws_config() {
        let config_content = r#"
[profile default]
region = us-east-1
output = json

[profile dev]
sso_account_id = 123456789012
sso_role_name = DeveloperAccess
region = us-west-2
sso_start_url = https://example.awsapps.com/start

[profile prod]
sso_account_id = 987654321098
sso_role_name = ReadOnlyAccess  
region = us-east-1
"#;

        let profiles = parse_aws_config(config_content).unwrap();

        assert_eq!(profiles.len(), 3);
        assert_eq!(profiles[0].name, "default");
        assert_eq!(profiles[1].name, "dev");
        assert_eq!(profiles[2].name, "prod");

        assert_eq!(profiles[1].get_account_id().unwrap(), "123456789012");
        assert_eq!(profiles[1].get_role_name().unwrap(), "DeveloperAccess");
    }
}
