use std::fmt;

use anyhow::{Context, Result};
use inquire::{InquireError, Select};

use crate::config::Profile;

pub fn select_profile(profiles: Vec<Profile>) -> Result<Option<String>> {
    let options = profiles.into_iter().map(ProfileOption).collect();
    match Select::new("Select AWS Profile:", options)
        .with_page_size(10)
        .with_help_message("↑↓ to move, enter to select, type to filter, esc to cancel")
        .prompt_skippable()
    {
        Ok(profile) => Ok(profile.map(|profile| profile.0.name)),
        Err(InquireError::OperationInterrupted) => Ok(None),
        Err(error) => Err(error).context("Profile selection failed"),
    }
}

struct ProfileOption(Profile);

impl fmt::Display for ProfileOption {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.name)?;
        if let Some(account) = self.0.get_account_id() {
            write!(f, " ({account})")?;
        }
        if let Some(region) = self.0.get_region() {
            write!(f, " [{region}]")?;
        }
        if let Some(role) = self.0.get_role_name() {
            write!(f, " {{{role}}}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn formats_only_display_metadata() {
        let profile = Profile {
            name: "dev".to_owned(),
            attributes: HashMap::from([
                ("sso_account_id".to_owned(), "123456789012".to_owned()),
                ("region".to_owned(), "us-west-2".to_owned()),
                ("sso_role_name".to_owned(), "DeveloperAccess".to_owned()),
                ("aws_secret_access_key".to_owned(), "secret".to_owned()),
            ]),
        };
        assert_eq!(
            ProfileOption(profile).to_string(),
            "dev (123456789012) [us-west-2] {DeveloperAccess}"
        );
    }

    #[test]
    fn formats_profiles_without_metadata() {
        let profile = Profile {
            name: "default".to_owned(),
            attributes: HashMap::new(),
        };
        assert_eq!(ProfileOption(profile).to_string(), "default");
    }
}
