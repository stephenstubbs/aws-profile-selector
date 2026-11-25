# AWS Profile Selector

A small Rust CLI for selecting AWS profiles with inline fuzzy search.

## Install

With Nix and flakes enabled:

```sh
nix profile add github:stephenstubbs/aws-profile-selector
aws-profile-selector --help
```

Or run without installing:

```sh
nix run github:stephenstubbs/aws-profile-selector -- --help
```

From a checkout with a current stable Rust toolchain:

```sh
cargo install --path . --locked
```

### Use from another flake

```nix
{
  inputs.aws-profile-selector = {
    url = "github:stephenstubbs/aws-profile-selector";
    inputs.nixpkgs.follows = "nixpkgs";
  };
}
```

Add `aws-profile-selector.packages.${system}.default` to your packages.
The self-contained `aws-profile-selector.overlays.default` also exposes
`pkgs.aws-profile-selector`.

## Usage

```sh
aws-profile-selector                 # Fuzzy-search configured profiles
aws-profile-selector -a dev          # Activate an existing profile
aws-profile-selector -n custom       # Save a name without checking configuration
aws-profile-selector -d              # Remove the saved selection
aws-profile-selector -c -a dev       # Print a command for this shell only
aws-profile-selector -c -d           # Print a command to unset AWS_PROFILE
aws-profile-selector -c -n dev --shell nu
```

`--activate`, `--new`, and `--deactivate` are mutually exclusive.
Profile names must be nonempty and contain no control characters or surrounding
whitespace. Use **↑/↓**, type to filter, and **Enter** to select.
**Esc** or **Ctrl+C** cancels without changing the saved selection.

Profiles come from `~/.aws/config` and `~/.aws/credentials`, including `default`
and credentials-only profiles. `AWS_CONFIG_FILE` and
`AWS_SHARED_CREDENTIALS_FILE` override those paths. Missing default files are
allowed; missing or unreadable explicit overrides are errors.
Only account ID, region, and SSO role metadata from config are displayed.

```ini
[default]
region = us-east-1

[profile dev]
sso_session = work
sso_account_id = 123456789012
sso_role_name = DeveloperAccess
region = us-west-2

[sso-session work]
sso_start_url = https://example.awsapps.com/start
sso_region = us-east-1
```

By default, the selection is saved atomically to `~/.aws/current-profile`.
The CLI cannot modify its parent shell: a shell integration must read that file.
`--current` instead prints a safely quoted shell command and never changes the
file. Syntax is detected from the basename of `$SHELL`; use `--shell posix`
(`sh`, `bash`, `zsh`), `--shell fish`, or `--shell nu` to override it.
Errors go to stderr, leaving stdout usable for command capture.

### Nushell integration

Add this to `config.nu`, replacing the path with your checkout:

```nu
source /path/to/aws-profile-selector/shell/awsps.nu
```

Nix packages also install the script at
`share/aws-profile-selector/awsps.nu`. For example, in a Home Manager module:

```nix
{ pkgs, ... }:
{
  home.packages = [ pkgs.aws-profile-selector ];
  programs.nushell = {
    enable = true;
    extraConfig = ''
      source ${pkgs.aws-profile-selector}/share/aws-profile-selector/awsps.nu
    '';
  };
}
```

This example assumes the overlay above is in `nixpkgs.overlays`.

```nu
awsps                    # Select and load a saved profile
awsps -a dev             # Activate and load dev
awsps -c -n temporary    # Override only this shell
awsps -c -d              # Disable only this shell, keeping the saved profile
awsps -d                 # Clear the saved profile and this shell's override
```

The script appends a pre-prompt hook without replacing existing hooks.
Current-shell overrides, including deactivation, survive subsequent prompts.
Generated values are decoded as data, not evaluated as Nushell code.

### POSIX shells and Fish

For a current-shell selection:

```sh
eval "$(aws-profile-selector --current --shell posix)"
```

```fish
aws-profile-selector --current --shell fish | source
```

## Development

The flake provides the latest stable Rust toolchain available in its lock,
including rustfmt, Clippy, rust-src, and rust-analyzer. Package builds use the
same toolchain. Nix supports Linux on x86-64 and ARM64, and macOS on Apple
Silicon. Nixpkgs 26.11 no longer supports Intel macOS.

```sh
direnv allow
direnv reload
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --all -- --check
cargo deny check advisories
nix flake check --print-build-logs
```

Flake checks build and test the release package, check Clippy and formatting,
and exercise Nushell integration and POSIX/Fish/Nushell quoting in isolation.

To refresh dependencies:

```sh
nix flake update
direnv reload
cargo upgrade --incompatible
cargo update
```

Review and retain both lockfiles for reproducible builds.

## License

MIT.
