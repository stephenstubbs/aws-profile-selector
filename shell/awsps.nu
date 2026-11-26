def --wrapped selection-record [...args: string] {
    let result = (do --capture-errors { ^aws-profile-selector ...$args } | from json)
    if ($result | columns) != ["profile"] {
        error make {msg:"Unsupported AWS selector metadata"}
    }
    if $result.profile != null and ($result.profile | describe) != "string" {
        error make {msg:"Unsupported AWS profile metadata"}
    }
    $result
}

def --env apply-selection [selection: record] {
    if $selection.profile == null {
        hide-env --ignore-errors AWS_PROFILE
    } else {
        $env.AWS_PROFILE = $selection.profile
    }
    $env.AWS_PROFILE_SELECTOR_LOADED = ($selection.profile | default "")
    $env.AWS_PROFILE_SELECTOR_LOAD_ERROR = "false"
}

export def --env load_aws_profile [] {
    if $env.AWS_PROFILE_CURRENT_SHELL? in [true "true" 1 "1" "yes"] {
        return
    }
    let current = ($env.AWS_PROFILE? | default "")
    let loaded = ($env.AWS_PROFILE_SELECTOR_LOADED? | default "")
    if $current != "" and $current != $loaded {
        return
    }
    let selection = (try {
        selection-record --show-current --context --json
    } catch {
        null
    })
    if $selection == null {
        if $env.AWS_PROFILE_SELECTOR_LOAD_ERROR? != "true" {
            print -e "AWS saved selection is unavailable; current shell metadata was preserved."
        }
        $env.AWS_PROFILE_SELECTOR_LOAD_ERROR = "true"
        return
    }
    apply-selection $selection
}

export def --env awsps-load [] {
    load_aws_profile
}

def option-flags [args: list<string>] {
    mut flags = []
    for arg in $args {
        if $arg == "--" {break}
        if ($arg | str starts-with "--") {
            $flags = ($flags | append ($arg | split row "=" | get 0))
        } else if ($arg | str starts-with "-") {
            for flag in ($arg | str substring 1.. | split chars) {
                let name = match $flag {
                    "c" => "--current"
                    "l" => "--list"
                    "h" => "--help"
                    "V" => "--version"
                    _ => $"-($flag)"
                }
                $flags = ($flags | append $name)
                if $flag in ["a" "n"] {break}
            }
        }
    }
    $flags
}

export def --env --wrapped awsps [...args: string] {
    if ($args | get -o 0) == "config" {
        return (do --capture-errors { ^aws-profile-selector ...$args })
    }
    let flags = (option-flags $args)
    if ($flags | any {|flag| $flag in ["--help" "--version" "--list" "--show-current"]}) {
        return (do --capture-errors { ^aws-profile-selector ...$args })
    }
    let extra = (
        (if "--context" in $flags {[]} else {["--context"]})
        ++ (if "--json" in $flags {[]} else {["--json"]})
    )
    let selection = (selection-record ...($extra ++ $args))
    apply-selection $selection
    $env.AWS_PROFILE_CURRENT_SHELL = "false"
    if "--current" in $flags {
        $env.AWS_PROFILE_CURRENT_SHELL = "true"
    }
}

$env.config.hooks.pre_prompt = (
    $env.config.hooks.pre_prompt | append {|| load_aws_profile}
)
load_aws_profile
