use std/assert

let existing_hooks = ($env.config.hooks.pre_prompt | length)
source ../shell/awsps.nu
assert equal ($env.config.hooks.pre_prompt | length) ($existing_hooks + 1)

awsps -n saved
assert equal $env.AWS_PROFILE saved

let unusual = "team's \"$dev\" \\ (prod); `whoami`"
awsps --current -n $unusual
assert equal $env.AWS_PROFILE $unusual
assert equal (open --raw ($nu.home-dir | path join ".aws" "current-profile")) saved
awsps-load
assert equal $env.AWS_PROFILE $unusual

awsps -c -d
assert (not ("AWS_PROFILE" in $env))
awsps-load
assert (not ("AWS_PROFILE" in $env))
assert equal (open --raw ($nu.home-dir | path join ".aws" "current-profile")) saved

awsps -n persistent
assert equal $env.AWS_PROFILE persistent
assert (not ("AWS_PROFILE_CURRENT_SHELL" in $env))

awsps -c -n temporary
try { awsps -a missing } catch { null }
assert equal $env.AWS_PROFILE temporary
assert ("AWS_PROFILE_CURRENT_SHELL" in $env)
try { awsps -c -a missing } catch { null }
assert equal $env.AWS_PROFILE temporary
assert ("AWS_PROFILE_CURRENT_SHELL" in $env)
awsps --help | ignore
awsps --version | ignore
assert equal $env.AWS_PROFILE temporary
assert ("AWS_PROFILE_CURRENT_SHELL" in $env)

awsps -d
assert (not ("AWS_PROFILE" in $env))
assert (not ("AWS_PROFILE_CURRENT_SHELL" in $env))

$env.AWS_PROFILE = "stale"
"" | save --force ($nu.home-dir | path join ".aws" "current-profile")
awsps-load
assert (not ("AWS_PROFILE" in $env))
