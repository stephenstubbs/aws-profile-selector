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
        .arg(
            Arg::new("new")
                .short('n')
                .long("new")
                .help("Set a profile name that is not available in the list")
                .value_name("PROFILE"),
        )
        .arg(
            Arg::new("current")
                .short('c')
                .long("current")
                .help("Output the profile name only (for setting in current shell)")
                .action(clap::ArgAction::SetTrue),
        )
        .get_matches();

    let current_profile_path = get_current_profile_path()?;
    let current_shell_mode = matches.get_flag("current");

    // Handle deactivation first
    if matches.get_flag("deactivate") {
        if current_shell_mode {
            // Output shell-specific unset command
            print_shell_command(None);
        } else {
            if current_profile_path.exists() {
                std::fs::remove_file(&current_profile_path)?;
                println!("AWS profile deactivated");
            } else {
                println!("No active AWS profile to deactivate");
            }
        }
        return Ok(());
    }

    // Handle new profile (doesn't require reading AWS config)
    if let Some(profile_name) = matches.get_one::<String>("new") {
        if current_shell_mode {
            // Output shell-specific export command
            print_shell_command(Some(profile_name));
        } else {
            // Create .aws directory if it doesn't exist
            if let Some(parent) = current_profile_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            
            // Write profile name to file
            std::fs::write(&current_profile_path, profile_name)?;
            println!("AWS profile activated: {profile_name}");
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
            if current_shell_mode {
                // Output shell-specific export command
                print_shell_command(Some(&profile_name));
            } else {
                // Create .aws directory if it doesn't exist
                if let Some(parent) = current_profile_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                
                // Write profile name to file
                std::fs::write(&current_profile_path, &profile_name)?;
                println!("AWS profile activated: {profile_name}");
            }
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

fn print_shell_command(profile_name: Option<&str>) {
    // Detect the shell from SHELL environment variable
    let shell = std::env::var("SHELL").unwrap_or_default();
    
    match profile_name {
        Some(name) => {
            if shell.contains("nu") || shell.contains("nushell") {
                // Nushell syntax
                print!("$env.AWS_PROFILE = \"{}\"", name);
            } else if shell.contains("fish") {
                // Fish syntax
                print!("set -gx AWS_PROFILE \"{}\"", name);
            } else {
                // Default to bash/zsh/POSIX syntax
                print!("export AWS_PROFILE=\"{}\"", name);
            }
        }
        None => {
            if shell.contains("nu") || shell.contains("nushell") {
                // Nushell syntax for unsetting
                print!("hide-env AWS_PROFILE");
            } else if shell.contains("fish") {
                // Fish syntax for unsetting
                print!("set -e AWS_PROFILE");
            } else {
                // Default to bash/zsh/POSIX syntax
                print!("unset AWS_PROFILE");
            }
        }
    }
}
