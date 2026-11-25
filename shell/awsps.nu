def --env awsps-load [] {
    if "AWS_PROFILE_CURRENT_SHELL" in $env {
        return
    }

    let path = ($nu.home-dir | path join ".aws" "current-profile")
    let profile = if ($path | path exists) { open --raw $path | str trim } else { "" }
    if ($profile | is-empty) {
        hide-env --ignore-errors AWS_PROFILE
    } else {
        $env.AWS_PROFILE = $profile
    }
}

def --env --wrapped awsps [--current (-c), ...args: string] {
    if $current {
        let command = (^aws-profile-selector --current --shell nu ...$args | str trim)
        if $env.LAST_EXIT_CODE != 0 {
            return
        }
        if $command == "hide-env --ignore-errors AWS_PROFILE" {
            hide-env --ignore-errors AWS_PROFILE
            $env.AWS_PROFILE_CURRENT_SHELL = "true"
        } else if ($command | str starts-with '$env.AWS_PROFILE = ') {
            $env.AWS_PROFILE = ($command | str replace '$env.AWS_PROFILE = ' '' | from nuon)
            $env.AWS_PROFILE_CURRENT_SHELL = "true"
        }
    } else {
        ^aws-profile-selector ...$args
        let informational = ($args | any {|arg| $arg in ["--help" "-h" "--version" "-V"] })
        if $env.LAST_EXIT_CODE == 0 and not $informational {
            hide-env --ignore-errors AWS_PROFILE_CURRENT_SHELL
            awsps-load
        }
    }
}

$env.config.hooks.pre_prompt = ($env.config.hooks.pre_prompt | append {|| awsps-load })
awsps-load
