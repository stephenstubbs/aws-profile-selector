use std/assert

def --env main [binary?: string] {
    let root = (^mktemp -d | str trim)
    let old_home = $env.HOME
    let old_path = $env.PATH
    $env.HOME = ($root | path join home)
    $env.XDG_CONFIG_HOME = ($env.HOME | path join .config)
    $env.XDG_CACHE_HOME = ($env.HOME | path join .cache)
    $env.XDG_STATE_HOME = ($env.HOME | path join .local state)
    mkdir ($env.HOME | path join .aws)
    $env.AWS_CONFIG_FILE = ($env.HOME | path join .aws config)
    $env.AWS_SHARED_CREDENTIALS_FILE = ($env.HOME | path join .aws credentials)
    "[profile fixture-a]\nregion=eu-west-1\n[profile fixture-b]\nregion=eu-west-2\n" | save $env.AWS_CONFIG_FILE
    "" | save $env.AWS_SHARED_CREDENTIALS_FILE
    if $binary != null {
        $env.PATH = ($env.PATH | prepend ($binary | path expand | path dirname))
    }
    for key in ["AWS_PROFILE" "AWS_PROFILE_CURRENT_SHELL" "AWS_PROFILE_SELECTOR_LOADED"
        "AWS_PROFILE_SELECTOR_LOAD_ERROR" "AWS_ACCESS_KEY_ID" "AWS_SECRET_ACCESS_KEY"
        "AWS_SESSION_TOKEN" "AWS_SECURITY_TOKEN" "AWS_WEB_IDENTITY_TOKEN_FILE"
        "AWS_ROLE_ARN" "AWS_DEFAULT_PROFILE" "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI"
        "AWS_CONTAINER_CREDENTIALS_FULL_URI" "AWS_CONTAINER_AUTHORIZATION_TOKEN"
        "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE"] {
        hide-env --ignore-errors $key
    }
    try {
        let hooks = ($env.config.hooks.pre_prompt | length)
        source ../shell/awsps.nu
        assert equal ($env.config.hooks.pre_prompt | length) ($hooks + 1)
        let current = ($env.HOME | path join .aws current-profile)

        awsps -a fixture-a
        assert equal $env.AWS_PROFILE fixture-a
        awsps -ca fixture-b
        assert equal $env.AWS_PROFILE fixture-b
        assert equal (open --raw $current) fixture-a
        awsps -l | ignore
        let saved = (awsps --show-current --json | from json)
        assert equal $saved.profile fixture-a
        assert equal $env.AWS_PROFILE fixture-b

        let failed = (try {awsps -a missing; false} catch {true})
        assert $failed
        assert equal $env.AWS_PROFILE fixture-b
        assert equal $env.AWS_PROFILE_CURRENT_SHELL "true"
        awsps -c -d
        awsps-load
        assert equal $env.AWS_PROFILE? null
        assert equal $env.AWS_PROFILE_CURRENT_SHELL "true"
        awsps -a fixture-a
        assert equal $env.AWS_PROFILE fixture-a
        assert equal $env.AWS_PROFILE_CURRENT_SHELL "false"

        $env.SHELL = "/bin/bash"
        awsps -c --context -a fixture-b
        assert equal $env.AWS_PROFILE fixture-b
        let unusual = "team's \"$dev\" \\ (prod); `whoami`"
        awsps -c -n $unusual --shell fish
        assert equal $env.AWS_PROFILE $unusual
        assert equal (open --raw $current) fixture-a
        awsps-load
        assert equal $env.AWS_PROFILE $unusual

        awsps -a fixture-a
        $env.AWS_PROFILE = "project-override"
        awsps-load
        assert equal $env.AWS_PROFILE project-override
        $env.AWS_PROFILE = "fixture-a"
        ^aws-profile-selector -a fixture-b | ignore
        awsps-load
        assert equal $env.AWS_PROFILE fixture-b

        "bad\nstate" | save --force $current
        awsps-load
        assert equal $env.AWS_PROFILE fixture-b
        awsps -a fixture-a
        awsps -c -a fixture-b
        let status = (awsps config status | from json)
        assert equal $status.sources 0
        assert equal $env.AWS_PROFILE fixture-b
        $env.AWS_ACCESS_KEY_ID = "fictional-key-canary"
        let conflict = (try {awsps -a fixture-a; false} catch {true})
        assert $conflict
        assert equal $env.AWS_PROFILE fixture-b
        hide-env AWS_ACCESS_KEY_ID

        let stub = ($root | path join stub)
        mkdir $stub
        "#!/bin/sh\nprintf '%s' '{\"profile\":\"trap\"}'\nexit 1\n" | save ($stub | path join aws-profile-selector)
        ^chmod 700 ($stub | path join aws-profile-selector)
        $env.PATH = ($env.PATH | prepend $stub)
        let rejected = (try {awsps -c -a fixture-a; false} catch {true})
        assert $rejected
        assert equal $env.AWS_PROFILE fixture-b
        $env.PATH = ($env.PATH | skip 1)

        awsps -c -d
        awsps-load
        assert equal $env.AWS_PROFILE? null
        awsps -d
        assert equal $env.AWS_PROFILE? null
        assert equal $env.AWS_PROFILE_CURRENT_SHELL "false"
        awsps -a fixture-a
        awsps -d
        awsps-load
        assert equal $env.AWS_PROFILE? null
        print "Nushell AWS selection checks passed"
    } catch {|err|
        $env.HOME = $old_home
        $env.PATH = $old_path
        rm -r $root
        error make $err
    }
    $env.HOME = $old_home
    $env.PATH = $old_path
    rm -r $root
}
