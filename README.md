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

### Bash and Nushell integration

Both canonical modules provide `awsps` and `load_aws_profile`, load the saved
selection at startup, and preserve existing prompt hooks.

```bash
source /path/to/aws-profile-selector/shell/awsps.bash
```

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

The Bash module is installed beside it as `share/aws-profile-selector/awsps.bash`.
This example assumes the overlay above is in `nixpkgs.overlays`.

These commands have the same meaning in both shells:

```sh
awsps                    # Select and load a saved profile
awsps -a dev             # Activate and load dev
awsps -c -n temporary    # Override only this shell
awsps -c -d              # Disable only this shell, keeping the saved profile
awsps -d                 # Clear the saved profile and this shell's override
```

The script appends a pre-prompt hook without replacing existing hooks.
Current-shell overrides, including deactivation, survive subsequent prompts.
Failures, cancellation, help, listing and metadata management do not discard an
override. A successful persistent selection resumes saved-default loading.
External `AWS_PROFILE` context, such as a direnv project override, is respected.
Generated values are decoded as data in Nu; Bash uses the selector's trusted
POSIX literal serializer. Neither module retrieves 1Password data or logs in.

The selector refuses profile activation/loading when conflicting AWS credential
environment variables or `AWS_DEFAULT_PROFILE` are present. Clear the originating
credential context first rather than silently selecting a profile that the SDK
will not actually use. Region and config-file overrides remain native settings.

Deactivation clears **selection**, not authentication. It does not delete SSO
caches, revoke issued credentials or disable AWS's default credential chain.

### Structured context

```sh
aws-profile-selector --show-current --json
# {"profile":"example-dev"} or {"profile":null}
aws-profile-selector --current --activate example-dev --context --json
aws-profile-selector --activate example-dev --json
```

Persistent context is emitted only after successful persistence. `--context`
with `--shell posix`, `fish` or `nu` emits that shell's assignment/unset syntax.
Legacy output is unchanged when neither `--context` nor `--json` is requested.
Packages advertise `awsProfileContextVersion = 1` and `awsProfileConfigVersion = 1`.

## Confidential SSO configuration distribution

Keep native AWS SSO. Synchronize approved **profile definitions**, not the entire
`.aws` directory. Authentication tokens, refresh state, role caches, history and
the selector's `current-profile` are never uploaded, copied or removed.

An SSO-only catalog uses normal AWS INI syntax:

```ini
[profile example-dev]
sso_session = example-login
sso_account_id = 111111111111
sso_role_name = Developer
region = eu-west-1

[sso-session example-login]
sso_start_url = https://example.invalid/start
sso_region = eu-west-1
sso_registration_scopes = sso:account:access
```

This example is fictional. Store the real catalog privately, outside a source
checkout, and upload it as a Document to the appropriate 1Password account/vault
through the trusted UI. Separate organizations can use separate Documents and
accounts. Every catalog must define its own referenced sessions. Duplicate
profile/session names across Documents are errors, never last-account fallback.

Bind explicit stable 1Password IDs (replace all placeholders):

```sh
aws-profile-selector config source add example --account ACCOUNT_ID --vault VAULT_ID --document DOCUMENT_ID
aws-profile-selector config sync --dry-run
aws-profile-selector config sync
aws sso login --profile example-dev
```

Binding a source does not read the vault. Bindings live in the private
`$XDG_CONFIG_HOME/aws-profile-selector/sources.json`, defaulting to
`~/.config/aws-profile-selector/sources.json`. A replacement requires
`config source add ... --replace`; removal uses `config source remove NAME`.
Removing a binding does not implicitly edit native config or log out.

Sync uses the human 1Password desktop integration and explicit account/vault/
Document IDs. It does not accept ambient service-account/Connect/session tokens
as fallback, relay terminal password input, or print raw document/diagnostic data.
Authorize the desktop integration first. Standalone builds use native `op`;
`AWS_PROFILE_SELECTOR_OP_EXECUTABLE` can select a trusted executable. Managed
NixOS integration pins the reviewed desktop CLI wrapper.

Sync publishes one validated, atomic managed section in `~/.aws/config`; unrelated
sections are preserved. Sources must contain SSO metadata only. Access keys,
session tokens, credential processes, executable pagers, unknown settings,
unresolved sessions and unsafe paths are rejected. Paths must be outside
workspaces and the Nix store; symlinks/hardlinks and foreign/writable files are
refused. Installed files/directories are private (0600/0700).

Existing matching profiles require explicit `config sync --adopt`. Adoption
accepts equivalent SSO metadata only, including legacy inline fields. It never
drops additional settings or silently changes account/role identity.
Explicit existing login-session names must also match. Changing an already
managed principal requires reviewed `--accept-identity-change`, independently
of `--adopt`.
Unmanaged references prevent destructive retirement of managed sessions/profiles.
Observed concurrent edits and source-binding changes refuse installation.

`AWS_CONFIG_FILE` selecting another destination requires an explicit, safe
`--target ABSOLUTE_PATH`. Sync does not change `HOME` or credential-cache paths.
`--dry-run` means no configuration writes: it still explicitly retrieves the
Documents and may request desktop authorization.

Offline validation/installation and metadata-only status are also available:

```sh
aws-profile-selector config validate --file /private/path/catalog.ini
aws-profile-selector config install --file /private/path/catalog.ini --dry-run
aws-profile-selector config install --file /private/path/catalog.ini --adopt
aws-profile-selector config status
aws-profile-selector config clear
```

`config clear` removes only the managed section, preserving caches, saved
selection, bindings and unmanaged settings. Its `--dry-run` and `--target`
options have the same write protections. No operation is run automatically on
`cd`, shell prompts, Nix evaluation, rebuilds or boot.

The local native configuration remains readable by the Unix user. 1Password
protects the canonical copy; this is not a same-UID sandbox. Configuration
restoration is separate from fresh native SSO login on each machine.

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
bash tests/bash.sh target/debug/aws-profile-selector
nu --no-config-file tests/nushell.nu target/debug/aws-profile-selector
python3 tests/interactive.py target/debug/aws-profile-selector
nix flake check --print-build-logs
```

Flake checks build and test the release package, check Clippy and formatting,
and exercise both integrations and POSIX/Fish/Nushell quoting in isolation.
Twelve bounded real-PTY cases cover visible interactive menus, persistence and
Escape/Ctrl-C cancellation in both modes and shells. Metadata tests use fake
1Password Documents and isolated homes, never real login/cache files.

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
