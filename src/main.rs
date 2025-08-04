mod config;
mod ui;

use anyhow::Result;
use clap::{Arg, Command};
use config::read_aws_config;
use ui::ProfileSelector;
use std::path::PathBuf;

fn main() -> Result<()> {
    let matches = Command::new("aws-profile-selector")
        .version("0.1.0")
        .author("AWS Profile Selector - Rust Edition")
        .about("Interactive AWS profile selector")
        .arg(
            Arg::new("activate")
                .short('a')
                .long("activate")
                .help("Activate a specific profile by name (skips interactive selection)")
                .value_name("PROFILE"),
        )
        .arg(
            Arg::new("deactivate")
                .short('d')
                .long("deactivate")
                .help("Deactivate AWS_PROFILE")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let current_profile_path = get_current_profile_path()?;

    // Handle deactivation first
    if matches.get_flag("deactivate") {
        if current_profile_path.exists() {
            std::fs::remove_file(&current_profile_path)?;
            println!("AWS profile deactivated");
        } else {
            println!("No active AWS profile to deactivate");
        }
        return Ok(());
    }

    let profiles = read_aws_config()?;

    if profiles.is_empty() {
        eprintln!("No AWS profiles found in ~/.aws/config");
        std::process::exit(1);
    }

    // Handle direct profile activation
    let selected_profile = if let Some(profile_name) = matches.get_one::<String>("activate") {
        // Validate that the profile exists
        if profiles.iter().any(|p| &p.name == profile_name) {
            Some(profile_name.clone())
        } else {
            eprintln!("Profile '{}' not found in AWS config", profile_name);
            eprintln!("Available profiles:");
            for profile in &profiles {
                eprintln!("  {}", profile.name);
            }
            std::process::exit(1);
        }
    } else {
        // Run interactive selector
        let mut selector = ProfileSelector::new(profiles);
        selector.run()?
    };

    match selected_profile {
        Some(profile_name) => {
            // Create .aws directory if it doesn't exist
            if let Some(parent) = current_profile_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            
            // Write profile name to file
            std::fs::write(&current_profile_path, &profile_name)?;
            println!("AWS profile activated: {profile_name}");
        }
        None => {
            println!("No profile selected");
            std::process::exit(1);
        }
    }

    Ok(())
}

fn get_current_profile_path() -> Result<PathBuf> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| anyhow::anyhow!("Unable to determine home directory"))?;
    
    Ok(home_dir.join(".aws").join("current-profile"))
}
