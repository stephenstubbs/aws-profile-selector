# AWS Profile Selector

A fast, interactive AWS profile selector CLI tool built in Rust.

## Features

- 🔍 **Fuzzy search** through AWS profiles
- ⚡ **Fast inline interface** - no full-screen takeover
- 🎯 **Arrow key navigation**
- 📦 **Single binary** with no runtime dependencies

## Installation

### Option 1: Direct Installation with Nix Profile
```bash
# Install directly from GitHub
nix profile install github:stephenstubbs/aws-profile-selector

# Verify installation
aws-profile-selector --help
```

### Option 2: Run Without Installing
```bash
# Run directly from GitHub
nix run github:stephenstubbs/aws-profile-selector -- --help
```

### Option 3: Add to Your Flake
Add to `flake.nix` inputs:
```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    aws-profile-selector = {
      url = "github:stephenstubbs/aws-profile-selector";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, aws-profile-selector, ... }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          aws-profile-selector.packages.${system}.default
        ];
      };
    };
}
```

### Option 4: Development Environment
```bash
# Clone and enter development environment
git clone https://github.com/stephenstubbs/aws-profile-selector
cd aws-profile-selector
nix develop

# Build and run
cargo build --release
./target/release/aws-profile-selector --help
```

## Usage

### Direct Usage

Run the selector directly:
```bash
./target/release/aws-profile-selector
```

**Interactive Mode (default):**
```bash
aws-profile-selector                    # Interactive selection
```

**Direct Profile Activation:**
```bash
aws-profile-selector -a dev             # Activate 'dev' profile directly
aws-profile-selector --activate prod    # Activate 'prod' profile directly
```

**Deactivate Profile:**
```bash
aws-profile-selector -d                 # Deactivate AWS_PROFILE
aws-profile-selector --deactivate       # Deactivate AWS_PROFILE
```

**Options:**
- `-a, --activate <PROFILE>`: Activate a specific profile by name (skips interactive selection)
- `-d, --deactivate`: Deactivate AWS_PROFILE

### Shell Integration (Nushell)

Add these functions and hooks to your nushell config (`~/.config/nushell/config.nu`):

```nu
def --env load_aws_profile [] {
    let current_profile_file = ([$env.HOME ".aws" "current-profile"] | path join)

    if ($current_profile_file | path exists) {
        let profile_name = (open $current_profile_file | str trim)
        if ($profile_name | is-not-empty) {
            $env.AWS_PROFILE = $profile_name
        }
    } else {
        if "AWS_PROFILE" in $env {
            hide-env AWS_PROFILE
        }
    }
}

# Set up hooks to automatically load AWS profile
$env.config = ($env.config | upsert hooks {
    pre_prompt: [
        { ||
            load_aws_profile
        }
    ]
    env_change: {
        PWD: [
            { |before, after|
                load_aws_profile
            }
        ]
    }
})
```

This configuration will:
- **Automatically load the AWS profile** when nushell starts
- **Re-load the AWS profile** every time you change directories (`cd`)
- **Re-load the AWS profile** before each prompt is displayed
- **Provide the `awsps` command** for interactive profile selection and management


## How It Works

1. **Reads your AWS config** from `~/.aws/config`
2. **Parses profile sections** and extracts metadata (account ID, region, role name)
3. **Presents an interactive list** with fuzzy search capabilities
4. **Stores the selected profile** in `~/.aws/current-profile`
5. **Nushell integration** reads this file to set `$env.AWS_PROFILE`

## Interface

- **↑/↓ arrows**: Navigate through profiles
- **Type**: Filter profiles with fuzzy search (no need to press `/`)
- **Enter**: Select the highlighted profile
- **Esc/q**: Cancel and exit

## AWS Config Format

The tool reads standard AWS config files. Example:

```ini
[profile dev]
sso_account_id = 123456789012
sso_role_name = DeveloperAccess
region = us-west-2
sso_start_url = https://example.awsapps.com/start

[profile prod]
sso_account_id = 987654321098
sso_role_name = ReadOnlyAccess
region = us-east-1
sso_start_url = https://example.awsapps.com/start
```

## License

MIT License
