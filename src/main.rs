mod cli;
mod config;
mod shell;
mod state;
mod ui;

use anyhow::{Context, Result, bail, ensure};
use clap::Parser;
use cli::Cli;
use shell::Shell;

fn main() -> Result<()> {
    let args = Cli::parse();

    if args.deactivate {
        if args.current {
            println!("{}", args.shell.unwrap_or_else(Shell::detect).command(None));
        } else if state::deactivate()? {
            println!("AWS profile deactivated");
        } else {
            println!("No active AWS profile to deactivate");
        }
        return Ok(());
    }

    let name = if let Some(name) = args.new_profile {
        name
    } else {
        let profiles = config::read_aws_config()?;
        ensure!(
            !profiles.is_empty(),
            "No AWS profiles found in AWS config or credentials files"
        );

        if let Some(name) = args.activate {
            if !profiles.iter().any(|profile| profile.name == name) {
                let available = profiles
                    .iter()
                    .map(|profile| profile.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!("Profile {name:?} not found. Available profiles: {available}");
            }
            name
        } else {
            ui::select_profile(profiles)?.context("No profile selected")?
        }
    };
    cli::validate_profile_name(&name).map_err(anyhow::Error::msg)?;

    if args.current {
        println!(
            "{}",
            args.shell
                .unwrap_or_else(Shell::detect)
                .command(Some(&name))
        );
    } else {
        state::activate(&name)?;
        println!("AWS profile activated: {name}");
    }
    Ok(())
}
