use std/assert

let name = "team's \"$dev\" \\ (prod); $(touch injected); `whoami`"

let posix = (^aws-profile-selector --current --new $name --shell posix | str trim)
let actual = (^sh -c ($posix + '; printf "%s" "$AWS_PROFILE"'))
assert equal $actual $name
let unset = (^aws-profile-selector --current --deactivate --shell posix | str trim)
let result = (^sh -c ($posix + '; ' + $unset + '; ' + $unset + '; test -z "${AWS_PROFILE+x}"') | complete)
assert equal $result.exit_code 0

let fish = (^aws-profile-selector --current --new $name --shell fish | str trim)
let actual = (^fish --no-config -c ($fish + '; printf "%s" "$AWS_PROFILE"'))
assert equal $actual $name
let unset = (^aws-profile-selector --current --deactivate --shell fish | str trim)
let result = (^fish --no-config -c ($fish + '; ' + $unset + '; ' + $unset + '; set -q AWS_PROFILE; and exit 1; exit 0') | complete)
assert equal $result.exit_code 0

let nu_command = (^aws-profile-selector --current --new $name --shell nu | str trim)
let actual = (^nu --no-config-file -c ($nu_command + '; $env.AWS_PROFILE | to nuon') | from nuon)
assert equal $actual $name
let unset = (^aws-profile-selector --current --deactivate --shell nu | str trim)
let result = (^nu --no-config-file -c ($nu_command + '; ' + $unset + '; ' + $unset + '; if "AWS_PROFILE" in $env { error make {msg: "still set"} }') | complete)
assert equal $result.exit_code 0

assert (not ("injected" | path exists))
