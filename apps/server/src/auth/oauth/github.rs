//! GitHub.
//!
//! # Two common pitfalls
//!
//! **The token endpoint answers form-encoded unless asked otherwise.** Without
//! `Accept: application/json` it returns `access_token=...&scope=...`, which a
//! JSON parser rejects with no useful message.
//!
//! **`GET /user` returns `email: null` whenever the address is private**, the
//! default for many accounts. The address must come from `GET /user/emails`,
//! which needs the `user:email` scope, and the right entry is the one both
//! primary and verified. Taking the first entry, or `primary` without
//! `verified`, hands accounts to whoever added an unconfirmed address.

use serde::Deserialize;

use super::{Identity, IdentityError};

/// Where the browser is sent.
pub const AUTHORIZE: &str = "https://github.com/login/oauth/authorize";

/// Where the code is exchanged.
pub const TOKEN: &str = "https://github.com/login/oauth/access_token";

/// The API root on github.com.
///
/// GitHub Enterprise Server serves the same API under the installation's own
/// host at `/api/v3`, which is why this is computed, not fixed.
pub const API: &str = "https://api.github.com";

/// This installation's endpoints.
#[must_use]
pub fn endpoints(base: Option<&str>) -> super::Endpoints {
    base.map_or_else(
        || super::Endpoints {
            authorize: AUTHORIZE.to_owned(),
            token: TOKEN.to_owned(),
        },
        |base| super::Endpoints {
            authorize: format!("{base}/login/oauth/authorize"),
            token: format!("{base}/login/oauth/access_token"),
        },
    )
}

/// Where its API is.
///
/// github.com uses a separate hostname; Enterprise Server uses the same one at
/// `/api/v3`.
#[must_use]
pub fn api(base: Option<&str>) -> String {
    base.map_or_else(|| API.to_owned(), |base| format!("{base}/api/v3"))
}

/// The name this provider is stored under.
const NAME: &str = "github";

/// GitHub requires one and answers `403` without it.
const USER_AGENT: &str = "tessera";

/// The fields wanted from `GET /user`.
#[derive(Debug, Deserialize)]
struct User {
    /// The numeric account id. Stable across renames, unlike the login.
    id: i64,
    /// The login, for choosing a username.
    login: String,
    /// The display name, when set.
    #[serde(default)]
    name: Option<String>,
}

/// One row of `GET /user/emails`.
#[derive(Debug, Deserialize)]
struct Address {
    email: String,
    primary: bool,
    verified: bool,
}

/// Reads who signed in.
///
/// # Errors
///
/// Returns [`IdentityError`] when GitHub cannot be reached, refuses, or
/// answers with something unexpected.
pub async fn identify(api: &str, access_token: &str) -> Result<Identity, IdentityError> {
    let client = super::client(NAME)?;
    let user: User = super::read(
        NAME,
        client
            .get(format!("{api}/user"))
            .bearer_auth(access_token)
            .header(reqwest::header::USER_AGENT, USER_AGENT),
    )
    .await?;

    // Always fetched separately: `GET /user` may not carry it, and a missing
    // address must be reported as such, not as a half-working sign-in.
    let addresses: Vec<Address> = super::read(
        NAME,
        client
            .get(format!("{api}/user/emails"))
            .bearer_auth(access_token)
            .header(reqwest::header::USER_AGENT, USER_AGENT),
    )
    .await
    // A token without `user:email` is refused here, not at the top. The sign-in
    // still fails, but as "no address" instead of "GitHub refused", which
    // points at the missing scope instead of leaving a mystery.
    .unwrap_or_default();

    let primary = addresses
        .into_iter()
        .find(|address| address.primary && address.verified);

    Ok(Identity {
        subject: user.id.to_string(),
        email: primary.as_ref().map(|address| address.email.clone()),
        email_verified: primary.is_some(),
        preferred_username: Some(user.login),
        name: user.name,
    })
}
