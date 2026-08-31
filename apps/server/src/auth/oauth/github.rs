//! GitHub.
//!
//! # The two things that catch people
//!
//! **The token endpoint answers form-encoded unless asked otherwise.** Without
//! `Accept: application/json` it returns `access_token=...&scope=...`, which a
//! JSON parser reads as a failure with no useful message.
//!
//! **`GET /user` answers `email: null` whenever the address is private**, which
//! is the default for a lot of accounts. The address has to come from
//! `GET /user/emails` instead, which needs the `user:email` scope, and the one
//! to take is the entry that is both primary and verified. Taking the first
//! entry, or taking `primary` without `verified`, hands out accounts to
//! whoever added an unconfirmed address.

use serde::Deserialize;

use super::{Identity, IdentityError};

/// Where the browser is sent.
pub const AUTHORIZE: &str = "https://github.com/login/oauth/authorize";

/// Where the code is exchanged.
pub const TOKEN: &str = "https://github.com/login/oauth/access_token";

/// The API root on github.com.
///
/// GitHub Enterprise Server serves the same API under the installation's own
/// name at `/api/v3`, which is why this is worked out rather than fixed.
pub const API: &str = "https://api.github.com";

/// Where this installation's endpoints are.
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
/// github.com puts it on its own hostname; an Enterprise Server puts it under
/// the same one at `/api/v3`.
#[must_use]
pub fn api(base: Option<&str>) -> String {
    base.map_or_else(|| API.to_owned(), |base| format!("{base}/api/v3"))
}

/// The name this provider is stored under.
const NAME: &str = "github";

/// GitHub asks for one and answers `403` without it.
const USER_AGENT: &str = "tessera";

/// What `GET /user` gives back, of what is wanted.
#[derive(Debug, Deserialize)]
struct User {
    /// The numeric account id. Stable across a rename, which a login is not.
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

/// Reads who just signed in.
///
/// # Errors
///
/// Returns [`IdentityError`] when GitHub cannot be reached, refuses, or
/// answers with something this does not understand.
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

    // Asked for separately and always, because `GET /user` cannot be trusted to
    // carry it and a missing address has to be reported as a missing address
    // rather than as a sign-in that half worked.
    let addresses: Vec<Address> = super::read(
        NAME,
        client
            .get(format!("{api}/user/emails"))
            .bearer_auth(access_token)
            .header(reqwest::header::USER_AGENT, USER_AGENT),
    )
    .await
    // A token without `user:email` is refused here rather than at the top. The
    // sign-in still fails, but as "no address" rather than as "GitHub refused",
    // which is the difference between a scope to add and a mystery.
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
