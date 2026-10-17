//! `tess login`: get an API token for this machine.
//!
//! Three ways, for three situations:
//!
//! - By approving it in a browser, the default. The browser need not be on this machine: the
//!   command shows a link and a short code, the person opens the link wherever they are signed in,
//!   a phone will do, and this side polls until they approve. That is what makes it work over SSH.
//! - With a password typed here, for when there is no browser to hand at all. An account that signs
//!   in only with a passkey or a provider has no password; it uses a browser, or a token.
//! - With a token made elsewhere, read from standard input, for scripts and CI.

use std::time::Duration;

use url::Url;

use crate::{
    Error, Result,
    client::{Client, DeviceSignIn, PasswordSignIn},
};

/// What a token from `tess login --password` may do, less what the account
/// itself may not: the same as one approved in a browser.
const TOKEN_PERMISSIONS: [&str; 3] = ["shares:create", "shares:read", "shares:delete"];

/// Where the person approves a sign-in, with the code filled in.
///
/// # Errors
///
/// For a server address that does not join, which only a bug produces.
pub fn approval_url(client: &Client, started: &DeviceSignIn) -> Result<Url> {
    let mut url = client.url("/device")?;
    url.query_pairs_mut()
        .append_pair("code", &started.user_code);
    Ok(url)
}

/// Polls until the sign-in is answered, and returns the token.
///
/// # Errors
///
/// [`Error::Usage`] when the person denied it or it ran out of time, and
/// whatever the server or the network says otherwise.
pub async fn wait_for_approval(client: &Client, started: &DeviceSignIn) -> Result<String> {
    let interval = Duration::from_secs(started.interval.max(1));
    loop {
        tokio::time::sleep(interval).await;
        match client.redeem_device_code(&started.device_code).await {
            Ok(token) => return Ok(token),
            Err(Error::Server {
                code: Some(code), ..
            }) => match code.as_str() {
                "device_authorization_pending" => {}
                "device_authorization_denied" => {
                    return Err(Error::Usage("the sign-in was denied".to_owned()));
                }
                "device_code_unusable" => {
                    return Err(Error::Usage(
                        "the sign-in was not approved in time; run `tess login` again".to_owned(),
                    ));
                }
                _ => {
                    return Err(Error::Protocol(format!(
                        "unexpected answer while waiting: {code}"
                    )));
                }
            },
            Err(error) => return Err(error),
        }
    }
}

/// Signs in with a password, issues a token, and signs the session out again.
///
/// `ask_code` is called when the account asks for a code from its
/// authenticator app.
///
/// # Errors
///
/// When the password or the code is wrong, or the account may do none of what
/// a token here is for.
pub async fn with_password(
    client: &Client,
    identifier: &str,
    password: &str,
    ask_code: impl FnOnce() -> Result<String>,
) -> Result<String> {
    let signed_in = match client.sign_in_with_password(identifier, password).await? {
        PasswordSignIn::NeedsCode { ticket } => {
            let code = ask_code()?;
            client.finish_password_sign_in(&ticket, code.trim()).await?
        }
        done @ PasswordSignIn::Done { .. } => done,
    };
    let PasswordSignIn::Done { cookie, account } = signed_in else {
        return Err(Error::Protocol("asked for a second code".to_owned()));
    };

    let permissions: Vec<&str> = TOKEN_PERMISSIONS
        .into_iter()
        .filter(|wanted| account.permissions.iter().any(|held| held == wanted))
        .collect();
    let token = if permissions.is_empty() {
        Err(Error::Usage(
            "this account may not upload or manage shares".to_owned(),
        ))
    } else {
        client
            .create_token(&cookie, &client_name(), &permissions)
            .await
    };
    // The session was only a way to the token; it should not outlive it. A
    // sign-out that fails leaves a session that expires on its own.
    client.sign_out(&cookie).await.ok();
    token
}

/// Longest name the server takes, which is also the token name's limit.
const MAX_CLIENT_NAME_LENGTH: usize = 64;

/// What this machine calls itself on the approval page and in the token list.
#[must_use]
pub fn client_name() -> String {
    let host = std::process::Command::new("hostname")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    let name = match host {
        Some(host) => format!("tessera CLI on {host}"),
        None => "tessera CLI".to_owned(),
    };
    name.chars().take(MAX_CLIENT_NAME_LENGTH).collect()
}
