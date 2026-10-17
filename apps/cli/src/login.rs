//! `tessera login`: get an API token by approving this machine in a browser.
//!
//! The browser need not be on this machine. The command shows a link and a
//! short code, the person opens the link wherever they are signed in, and this
//! side polls until they approve. That is what makes it work over SSH.

use std::time::Duration;

use url::Url;

use crate::{
    Error, Result,
    client::{Client, DeviceSignIn},
};

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
                        "the sign-in was not approved in time; run `tessera login` again"
                            .to_owned(),
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
