# Metadata-only AWS profile selection for Bash.
_aws_flags() {
    _aws_current=0
    _aws_information=0
    local arg chars index char
    for arg in "$@"; do
        case $arg in
            --) break ;;
            --current) _aws_current=1 ;;
            --help|--version|--list|--show-current) _aws_information=1 ;;
            --*) ;;
            -?*)
                chars=${arg#-}
                for ((index=0; index<${#chars}; index++)); do
                    char=${chars:index:1}
                    case $char in
                        c) _aws_current=1 ;;
                        h|V|l) _aws_information=1 ;;
                        a|n) break ;;
                    esac
                done
                ;;
        esac
    done
}

_aws_selection_args() {
    local skip=0 literal=0 arg
    for arg in "$@"; do
        if ((literal)); then
            printf '%s\n' "$arg"
            continue
        fi
        if ((skip)); then
            skip=0
            continue
        fi
        case $arg in
            --) literal=1; printf '%s\n' "$arg" ;;
            --shell) skip=1 ;;
            --shell=*|--json|--context) ;;
            *) printf '%s\n' "$arg" ;;
        esac
    done
}

_aws_apply() {
    local output=$1
    case $output in
        "unset AWS_PROFILE"|"export AWS_PROFILE="*) ;;
        *) printf 'Unsupported AWS selector metadata\n' >&2; return 1 ;;
    esac
    # The trusted selector quotes the complete profile as a POSIX literal.
    eval "$output" || return
    export AWS_PROFILE_SELECTOR_LOADED="${AWS_PROFILE-}"
    unset AWS_PROFILE_SELECTOR_LOAD_ERROR
}

load_aws_profile() {
    case ${AWS_PROFILE_CURRENT_SHELL-} in
        true|1|yes) return 0 ;;
    esac
    if [[ -n ${AWS_PROFILE-} && ${AWS_PROFILE-} != "${AWS_PROFILE_SELECTOR_LOADED-}" ]]; then
        return 0
    fi
    local output
    if output=$(aws-profile-selector --show-current --context --shell posix 2>/dev/null); then
        _aws_apply "$output"
    else
        if [[ ${AWS_PROFILE_SELECTOR_LOAD_ERROR-} != true ]]; then
            printf 'AWS saved selection is unavailable; current shell metadata was preserved.\n' >&2
        fi
        export AWS_PROFILE_SELECTOR_LOAD_ERROR=true
    fi
    return 0
}

awsps() {
    if [[ ${1-} == config ]]; then
        aws-profile-selector "$@"
        return $?
    fi
    local _aws_current=0 _aws_information=0
    _aws_flags "$@"
    if ((_aws_information)); then
        aws-profile-selector "$@"
        return $?
    fi
    local -a args=(--context --shell posix)
    local arg output
    while IFS= read -r arg; do
        args+=("$arg")
    done < <(_aws_selection_args "$@")
    if output=$(aws-profile-selector "${args[@]}"); then
        _aws_apply "$output" || return
    else
        return $?
    fi
    if ((_aws_current)); then
        export AWS_PROFILE_CURRENT_SHELL=true
    else
        unset AWS_PROFILE_CURRENT_SHELL
    fi
}

_aws_profile_prompt() {
    local previous=$?
    load_aws_profile
    return "$previous"
}

if [[ $- == *i* ]]; then
    if declare -p PROMPT_COMMAND 2>/dev/null | grep -q 'declare -a'; then
        _aws_hook_present=0
        for _aws_hook in "${PROMPT_COMMAND[@]}"; do
            [[ $_aws_hook == _aws_profile_prompt ]] && _aws_hook_present=1
        done
        ((_aws_hook_present)) || PROMPT_COMMAND+=(_aws_profile_prompt)
        unset _aws_hook_present _aws_hook
    else
        case ";${PROMPT_COMMAND-};" in
            *";_aws_profile_prompt;"*) ;;
            *) PROMPT_COMMAND="_aws_profile_prompt${PROMPT_COMMAND:+;$PROMPT_COMMAND}" ;;
        esac
    fi
fi
load_aws_profile
