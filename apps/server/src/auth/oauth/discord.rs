//! Discord.
//!
//! # What to key on
//!
//! The snowflake in `id`, never `username`. Discord's own documentation says a
//! username is "not unique across the platform", and it can be changed.
//!
//! # The address
//!
//! `GET /users/@me` carries `email` and `verified`, both behind the `email`
//! scope, and `email` may be null even with it. `verified` is what says the
//! address belongs to the person rather than merely having been typed in.

use serde::Deserialize;

use super::{Identity, IdentityError};

/// Where the browser is sent.
pub const AUTHORIZE: &str = "https://discord.com/oauth2/authorize";

/// Where the code is exchanged.
pub const TOKEN: &str = "https://discord.com/api/oauth2/token";

/// The API root. A parameter at the call site so a test can point it elsewhere.
pub const API: &str = "https://discord.com/api";

/// The name this provider is stored under.
const NAME: &str = "discord";

/// What `GET /users/@me` gives back, of what is wanted.
#[derive(Debug, Deserialize)]
struct User {
    /// The snowflake. Stable, unlike the username.
    id: String,
    /// The username, for choosing one here.
    username: String,
    /// The display name, when set.
    #[serde(default)]
    global_name: Option<String>,
    /// Behind the `email` scope, and null even then for some accounts.
    #[serde(default)]
    email: Option<String>,
    /// Whether Discord says it checked the address.
    #[serde(default)]
    verified: bool,
}

/// Reads who just signed in.
///
/// # Errors
///
/// Returns [`IdentityError`] when Discord cannot be reached, refuses, or
/// answers with something this does not understand.
pub async fn identify(api: &str, access_token: &str) -> Result<Identity, IdentityError> {
    let client = super::client(NAME)?;
    let user: User = super::read(
        NAME,
        client
            .get(format!("{api}/users/@me"))
            .bearer_auth(access_token),
    )
    .await?;

    // An unverified address is dropped rather than carried with a flag. Every
    // caller would have to remember to check it, and forgetting is the account
    // takeover this exists to prevent.
    let verified = user.verified;
    Ok(Identity {
        subject: user.id,
        email: user.email.filter(|_| verified),
        email_verified: verified,
        preferred_username: Some(user.username),
        name: user.global_name,
    })
}
