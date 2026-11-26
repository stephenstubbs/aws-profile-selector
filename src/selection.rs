use anyhow::{Result, bail};
use serde::Serialize;

#[derive(Serialize)]
struct Context<'a> {
    profile: Option<&'a str>,
}

pub fn json(profile: Option<&str>) -> Result<String> {
    Ok(serde_json::to_string(&Context { profile })?)
}

pub fn check_environment() -> Result<()> {
    let variables = [
        "AWS_ACCESS_KEY_ID",
        "AWS_SECRET_ACCESS_KEY",
        "AWS_SESSION_TOKEN",
        "AWS_SECURITY_TOKEN",
        "AWS_WEB_IDENTITY_TOKEN_FILE",
        "AWS_ROLE_ARN",
        "AWS_CONTAINER_CREDENTIALS_RELATIVE_URI",
        "AWS_CONTAINER_CREDENTIALS_FULL_URI",
        "AWS_CONTAINER_AUTHORIZATION_TOKEN",
        "AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE",
        "AWS_DEFAULT_PROFILE",
    ];
    let conflicting: Vec<_> = variables
        .into_iter()
        .filter(|key| std::env::var_os(key).is_some_and(|value| !value.is_empty()))
        .collect();
    if !conflicting.is_empty() {
        bail!(
            "conflicting AWS authentication environment is set ({}); clear that credential context before selecting a profile",
            conflicting.join(", ")
        );
    }
    Ok(())
}
