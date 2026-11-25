use clap::{ArgGroup, Parser};

use crate::shell::Shell;

#[derive(Debug, Parser)]
#[command(version, about)]
#[command(group(ArgGroup::new("action").args(["activate", "new_profile", "deactivate"])))]
pub struct Cli {
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

    /// Shell syntax for --current (otherwise detected from SHELL)
    #[arg(long, value_enum, requires = "current")]
    pub shell: Option<Shell>,
}

pub fn validate_profile_name(name: &str) -> Result<(), &'static str> {
    if name.is_empty() || name.trim() != name || name.chars().any(char::is_control) {
        Err("Profile names must be nonempty, with no control characters or surrounding whitespace")
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
