use crate::config::Profile;
use anyhow::Result;
use inquire::{InquireError, Select};

pub struct ProfileSelector {
    profiles: Vec<Profile>,
}

impl ProfileSelector {
    pub fn new(profiles: Vec<Profile>) -> Self {
        Self { profiles }
    }

    pub fn run(&mut self) -> Result<Option<String>> {
        if self.profiles.is_empty() {
            return Ok(None);
        }

        let options: Vec<String> = self.profiles.iter().map(format_profile_display).collect();

        let ans = Select::new("Select AWS Profile:", options)
            .with_page_size(10)
            .with_help_message("↑↓ to move, enter to select, type to filter")
            .prompt();

        match ans {
            Ok(selected_display) => {
                // Find the profile that matches the selected display string
                let selected_profile = self
                    .profiles
                    .iter()
                    .find(|profile| format_profile_display(profile) == selected_display)
                    .map(|profile| profile.name.clone());

                Ok(selected_profile)
            }
            Err(InquireError::OperationCanceled) => Ok(None),
            Err(InquireError::OperationInterrupted) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Selection failed: {}", e)),
        }
    }
}

fn format_profile_display(profile: &Profile) -> String {
    let mut parts = vec![profile.name.clone()];

    if let Some(account_id) = profile.get_account_id() {
        parts.push(format!("({account_id})"));
    }

    if let Some(region) = profile.get_region() {
        parts.push(format!("[{region}]"));
    }

    if let Some(role) = profile.get_role_name() {
        parts.push(format!("{{{role}}}"));
    }

    parts.join(" ")
}
