use std::{env, path::Path};

use clap::ValueEnum;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    #[value(aliases = ["sh", "bash", "zsh"])]
    Posix,
    Fish,
    #[value(alias = "nushell")]
    Nu,
}

impl Shell {
    pub fn detect() -> Self {
        env::var_os("SHELL")
            .as_deref()
            .and_then(|path| Path::new(path).file_stem())
            .and_then(|name| name.to_str())
            .map_or(Self::Posix, Self::from_name)
    }

    fn from_name(name: &str) -> Self {
        match name {
            "nu" | "nushell" => Self::Nu,
            "fish" => Self::Fish,
            _ => Self::Posix,
        }
    }

    pub fn command(self, profile: Option<&str>) -> String {
        match (self, profile) {
            (Self::Posix, Some(name)) => {
                format!("export AWS_PROFILE='{}'", name.replace('\'', "'\"'\"'"))
            }
            (Self::Fish, Some(name)) => format!(
                "set -gx AWS_PROFILE '{}'",
                name.replace('\\', "\\\\").replace('\'', "\\'")
            ),
            (Self::Nu, Some(name)) => format!(
                "$env.AWS_PROFILE = \"{}\"",
                name.replace('\\', "\\\\").replace('"', "\\\"")
            ),
            (Self::Posix, None) => "unset AWS_PROFILE".to_owned(),
            (Self::Fish, None) => "set -e AWS_PROFILE".to_owned(),
            (Self::Nu, None) => "hide-env --ignore-errors AWS_PROFILE".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_shell_metacharacters() {
        let name = r#"team's "$dev" \ (prod); `whoami`"#;
        assert_eq!(
            Shell::Posix.command(Some(name)),
            r#"export AWS_PROFILE='team'"'"'s "$dev" \ (prod); `whoami`'"#
        );
        assert_eq!(
            Shell::Fish.command(Some(name)),
            r#"set -gx AWS_PROFILE 'team\'s "$dev" \\ (prod); `whoami`'"#
        );
        assert_eq!(
            Shell::Nu.command(Some(name)),
            r#"$env.AWS_PROFILE = "team's \"$dev\" \\ (prod); `whoami`""#
        );
    }

    #[test]
    fn detects_only_exact_shell_names() {
        assert_eq!(Shell::from_name("nu"), Shell::Nu);
        assert_eq!(Shell::from_name("nushell"), Shell::Nu);
        assert_eq!(Shell::from_name("fish"), Shell::Fish);
        for name in ["bash", "zsh", "unknown", "nuclear", "fishy"] {
            assert_eq!(Shell::from_name(name), Shell::Posix);
        }
    }

    #[test]
    fn deactivation_commands_are_shell_specific() {
        assert_eq!(Shell::Posix.command(None), "unset AWS_PROFILE");
        assert_eq!(Shell::Fish.command(None), "set -e AWS_PROFILE");
        assert_eq!(
            Shell::Nu.command(None),
            "hide-env --ignore-errors AWS_PROFILE"
        );
    }
}
