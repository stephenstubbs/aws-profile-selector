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

**Set New Profile (not in config):**
```bash
aws-profile-selector -n custom          # Set 'custom' profile (even if not in AWS config)
aws-profile-selector --new temp-profile # Set 'temp-profile' profile
```

**Deactivate Profile:**
```bash
aws-profile-selector -d                 # Deactivate AWS_PROFILE
aws-profile-selector --deactivate       # Deactivate AWS_PROFILE
```

**Set Profile for Current Shell Only:**
```bash
# For current shell session only (doesn't write to ~/.aws/current-profile)
aws-profile-selector -c                 # Interactive selection, outputs shell command
aws-profile-selector -c -a dev          # Outputs: $env.AWS_PROFILE = "dev"
aws-profile-selector -c -n custom       # Outputs: $env.AWS_PROFILE = "custom"
aws-profile-selector -c -d              # Outputs: hide-env AWS_PROFILE
```

**Options:**
- `-a, --activate <PROFILE>`: Activate a specific profile by name (skips interactive selection)
- `-n, --new <PROFILE>`: Set a profile name that is not available in the list
- `-c, --current`: Output shell commands for current shell only (doesn't write to file)
- `-d, --deactivate`: Deactivate AWS_PROFILE

### Shell Integration (Nushell)

Add these functions and hooks to your nushell config (`~/.config/nushell/config.nu`):

#### Option 1: Using --wrapped flag (Recommended for Nushell 0.91.0+)
```nu
def --env --wrapped awsps [...args] {
    # Check if -c or --current is in the arguments
    let is_current = ($args | any {|arg| $arg == "-c" or $arg == "--current"})
    
    if $is_current {
        # Interactive mode with -c: run and capture output to set env var
        let cmd = (^aws-profile-selector ...$args | str trim)
        
        if ($cmd | is-not-empty) {
            if ($cmd | str contains '$env.AWS_PROFILE') {
                # Extract the profile name from: $env.AWS_PROFILE = "profile-name"
                let parts = ($cmd | parse '$env.AWS_PROFILE = "{profile}"')
                if ($parts | length) > 0 {
                    let profile = ($parts | first | get profile)
                    $env.AWS_PROFILE = $profile
                    $env.AWS_PROFILE_CURRENT_SHELL = "true"
                    print $"AWS_PROFILE set to ($profile) for current shell"
                }
            } else if ($cmd == 'hide-env AWS_PROFILE') {
                hide-env AWS_PROFILE
                if "AWS_PROFILE_CURRENT_SHELL" in $env {
                    hide-env AWS_PROFILE_CURRENT_SHELL
                }
                print "AWS_PROFILE unset for current shell"
            }
        }
    } else {
        # Non-current mode: clear current shell flag and pass through
        if "AWS_PROFILE_CURRENT_SHELL" in $env {
            hide-env AWS_PROFILE_CURRENT_SHELL
        }
        ^aws-profile-selector ...$args
    }
}

# Usage examples:
# awsps                      # Interactive selection (writes to file, hooks handle it)
# awsps -a dev              # Activate 'dev' profile (writes to file, hooks handle it)
# awsps -d                  # Deactivate profile (removes file, hooks handle it)
# awsps -c                  # Interactive selection for current shell only
# awsps -c -a dev           # Activate 'dev' for current shell only
# awsps -c -d               # Deactivate for current shell only
# awsps --current -n custom # Set custom profile for current shell only
# awsps --help              # Show help for aws-profile-selector
```

**Note:** The `--wrapped` flag allows the function to receive all arguments without Nushell intercepting flags like `--help`. This provides seamless pass-through of all arguments to the underlying `aws-profile-selector` command.

#### Option 2: Persistent Profile (File-based)
```nu
def --env load_aws_profile [] {
    if "AWS_PROFILE_CURRENT_SHELL" in $env {
        return
    }

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
