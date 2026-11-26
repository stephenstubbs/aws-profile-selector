mod cli;
mod config;
mod shell;
mod state;
mod ui;

use anyhow::{Context, Result, bail, ensure};
use aws_profile_selector::selection;
use clap::Parser;
use cli::Cli;
use shell::Shell;

fn main() -> Result<()> {
    let mut args = Cli::parse();
    if let Some(cli::Management::Config { command }) = args.command.take() {
        return aws_profile_selector::metadata::command::execute(command);
    }
    if args.list {
        for profile in checked_profiles()? {
            println!("{}", profile.name);
        }
        return Ok(());
    }
    if !args.deactivate {
        selection::check_environment()?;
    }
    if args.show_current {
        let name = state::current()?;
        if args.json || args.context {
            emit(&args, name.as_deref())?;
        } else if let Some(name) = name {
            println!("{name}");
        }
        return Ok(());
    }

    if args.deactivate {
        let removed = if args.current {
            false
        } else {
            state::deactivate()?
        };
        if args.current || args.context || args.json {
            emit(&args, None)?;
        } else {
            println!(
                "{}",
                if removed {
                    "AWS profile deactivated"
                } else {
                    "No active AWS profile to deactivate"
                }
            );
        }
        return Ok(());
    }

    let name = if let Some(name) = &args.new_profile {
        name.clone()
    } else {
        let profiles = checked_profiles()?;
        ensure!(
            !profiles.is_empty(),
            "No AWS profiles found in AWS config or credentials files"
        );

        if let Some(name) = &args.activate {
            if !profiles.iter().any(|profile| &profile.name == name) {
                let available = profiles
                    .iter()
                    .map(|profile| profile.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!("Profile {name:?} not found. Available profiles: {available}");
            }
            name.clone()
        } else {
            ui::select_profile(profiles)?.context("No profile selected")?
        }
    };
    cli::validate_profile_name(&name).map_err(anyhow::Error::msg)?;

    if !args.current {
        state::activate(&name)?;
    }
    if args.current || args.context || args.json {
        emit(&args, Some(&name))?;
    } else {
        println!("AWS profile activated: {name}");
    }
    Ok(())
}

fn emit(args: &Cli, profile: Option<&str>) -> Result<()> {
    if args.json {
        println!("{}", selection::json(profile)?);
    } else {
        println!(
            "{}",
            args.shell.unwrap_or_else(Shell::detect).command(profile)
        );
    }
    Ok(())
}

fn checked_profiles() -> Result<Vec<config::Profile>> {
    let profiles = config::read_aws_config()?;
    ensure!(
        profiles.iter().all(|profile| {
            cli::validate_profile_name(&profile.name).is_ok()
                && profile
                    .attributes
                    .values()
                    .all(|value| !value.chars().any(char::is_control))
        }),
        "configured AWS profile metadata contains invalid characters"
    );
    Ok(profiles)
}
