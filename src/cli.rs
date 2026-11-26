use aws_profile_selector::metadata::command::ConfigCommand;
use clap::{ArgGroup, Parser, Subcommand};

use crate::shell::Shell;

#[derive(Debug, Parser)]
#[command(version, about)]
#[command(args_conflicts_with_subcommands = true)]
#[command(group(ArgGroup::new("action").args(["activate", "new_profile", "deactivate", "show_current", "list"])))]
#[command(group(ArgGroup::new("shell_output").args(["current", "context"]).multiple(true)))]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Management>,
    /// Activate an existing AWS profile without interactive selection
    #[arg(short, long, value_name = "PROFILE", value_parser = parse_profile_name)]
    pub activate: Option<String>,

    /// Set a profile name without checking AWS configuration
    #[arg(short = 'n', long = "new", value_name = "PROFILE", value_parser = parse_profile_name)]
    pub new_profile: Option<String>,

    /// Deactivate AWS_PROFILE
    #[arg(short, long)]
    pub deactivate: bool,

    /// Print a shell command without changing the saved profile
    #[arg(short, long)]
    pub current: bool,

    /// Print saved selection without creating or changing state
    #[arg(long, conflicts_with = "current")]
    pub show_current: bool,

    /// List native profile names without selecting one
    #[arg(short, long, conflicts_with_all = ["current", "json", "context"])]
    pub list: bool,

    /// Emit selection metadata rather than a human-readable activation message
    #[arg(long)]
    pub context: bool,

    /// Emit {"profile":NAME_OR_NULL}; selection and persistence are one operation
    #[arg(long)]
    pub json: bool,

    /// Shell syntax for --current or --context (otherwise detected from SHELL)
    #[arg(long, value_enum, requires = "shell_output")]
    pub shell: Option<Shell>,
}

#[derive(Debug, Subcommand)]
pub enum Management {
    /// Explicit SSO metadata setup and synchronization, separate from selection and login
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

pub fn validate_profile_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty()
        || name.len() > 512
        || name.trim() != name
        || name.chars().any(char::is_control)
    {
        Err(
            "Profile names must contain 1..512 bytes, with no control characters or surrounding whitespace",
        )
    } else {
        Ok(())
    }
}

fn parse_profile_name(name: &str) -> Result<String, &'static str> {
    validate_profile_name(name)?;
    Ok(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn command_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn actions_are_mutually_exclusive() {
        for args in [
            vec!["awsps", "-a", "dev", "-n", "new"],
            vec!["awsps", "-a", "dev", "-d"],
            vec!["awsps", "-n", "new", "-d"],
        ] {
            assert_eq!(
                Cli::try_parse_from(args).unwrap_err().kind(),
                clap::error::ErrorKind::ArgumentConflict
            );
        }
    }

    #[test]
    fn validates_profile_names() {
        for name in ["", " ", " dev", "dev ", "a\nb", "a\0b", "a\u{1b}b"] {
            assert!(Cli::try_parse_from(["awsps", "-n", name]).is_err());
        }
        assert!(Cli::try_parse_from(["awsps", "-n", "team's $dev \"profile\""]).is_ok());
    }

    #[test]
    fn shell_requires_current_mode() {
        assert!(Cli::try_parse_from(["awsps", "--shell", "nu"]).is_err());
        let args = Cli::try_parse_from(["awsps", "-cd", "--shell", "nu"]).unwrap();
        assert_eq!(args.shell, Some(Shell::Nu));
    }
}
