#!/usr/bin/env bash
set -euo pipefail
binary=${1:?selector executable is required}
binary=$(cd "$(dirname "$binary")" && pwd -P)/$(basename "$binary")
repo=$(cd "$(dirname "$0")/.." && pwd -P)
work=$(mktemp -d "${TMPDIR:-/tmp/opencode}/awsps-bash.XXXXXXXX")
trap 'rm -rf -- "$work"' EXIT
mkdir -p "$work/bin" "$work/home/.aws"
ln -s "$binary" "$work/bin/aws-profile-selector"
export HOME=$work/home PATH=$work/bin:$PATH
export XDG_CONFIG_HOME=$HOME/.config XDG_CACHE_HOME=$HOME/.cache XDG_STATE_HOME=$HOME/.local/state
export AWS_CONFIG_FILE=$HOME/.aws/config AWS_SHARED_CREDENTIALS_FILE=$HOME/.aws/credentials
unset AWS_PROFILE AWS_PROFILE_CURRENT_SHELL AWS_PROFILE_SELECTOR_LOADED AWS_PROFILE_SELECTOR_LOAD_ERROR
unset AWS_ACCESS_KEY_ID AWS_SECRET_ACCESS_KEY AWS_SESSION_TOKEN AWS_SECURITY_TOKEN
unset AWS_WEB_IDENTITY_TOKEN_FILE AWS_ROLE_ARN AWS_DEFAULT_PROFILE
unset AWS_CONTAINER_CREDENTIALS_RELATIVE_URI AWS_CONTAINER_CREDENTIALS_FULL_URI
unset AWS_CONTAINER_AUTHORIZATION_TOKEN AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE
printf '[profile fixture-a]\nregion=eu-west-1\n[profile fixture-b]\nregion=eu-west-2\n' > "$AWS_CONFIG_FILE"
: > "$AWS_SHARED_CREDENTIALS_FILE"
# shellcheck source=shell/awsps.bash
source "$repo/shell/awsps.bash"
cd "$work"

check() { [[ $1 == "$2" ]] || { printf 'FAIL: %s\n' "$3" >&2; exit 1; }; }

awsps -a fixture-a
check "${AWS_PROFILE-}" fixture-a "persistent activation"
check "${AWS_PROFILE_CURRENT_SHELL-}" "" "persistent activation retained override"
awsps -ca fixture-b
check "${AWS_PROFILE-}" fixture-b "combined current flags"
check "$(cat "$HOME/.aws/current-profile")" fixture-a "current activation persisted"
awsps -l >/dev/null
awsps --show-current --json >/dev/null
check "${AWS_PROFILE-}" fixture-b "information changed override"
if awsps -a missing 2>/dev/null; then exit 1; fi
check "${AWS_PROFILE-}" fixture-b "failed persistent activation changed selection"
check "${AWS_PROFILE_CURRENT_SHELL-}" true "failed activation cleared override"
awsps -c -d
load_aws_profile
check "${AWS_PROFILE-}" "" "deactivation revived a saved profile"
check "${AWS_PROFILE_CURRENT_SHELL-}" true "deactivation lost explicit override"
awsps -a fixture-a
check "${AWS_PROFILE-}" fixture-a "successful selection after failure"
awsps -c -a fixture-b --json --shell fish
check "${AWS_PROFILE-}" fixture-b "caller output flags were not normalized"

adversarial='team'"'"'s "$dev" $(touch injected) `id`'
awsps -c -n "$adversarial"
check "${AWS_PROFILE-}" "$adversarial" "profile contents were evaluated"
[[ ! -e injected ]] || exit 1
awsps -a fixture-a
export AWS_PROFILE=project-override
load_aws_profile
check "$AWS_PROFILE" project-override "prompt replaced project context"
export AWS_PROFILE=fixture-a
aws-profile-selector -a fixture-b >/dev/null
load_aws_profile
check "$AWS_PROFILE" fixture-b "saved default did not refresh"

printf 'broken\033state' > "$HOME/.aws/current-profile"
load_aws_profile 2>/dev/null
check "$AWS_PROFILE" fixture-b "corrupt saved metadata selected a fallback"
awsps -a fixture-a
awsps -c -a fixture-b
awsps config status >/dev/null
check "$AWS_PROFILE" fixture-b "metadata management changed selection"
export AWS_ACCESS_KEY_ID=fictional-key-canary
if awsps -a fixture-a 2>/dev/null; then exit 1; fi
check "$AWS_PROFILE" fixture-b "conflicting credential environment changed selection"
unset AWS_ACCESS_KEY_ID
awsps -c -d
load_aws_profile
check "${AWS_PROFILE-}" "" "current deactivation failed"
awsps -d
check "${AWS_PROFILE_CURRENT_SHELL-}" "" "persistent deactivation retained override"
check "${AWS_PROFILE-}" "" "persistent deactivation kept profile"
printf 'Bash AWS selection checks passed\n'
