//! Turning a request into the account that made it.
//!
//! Three extractors, so a handler's signature states what it needs and the
//! compiler enforces it:
//!
//! - [`CurrentUser`]: signed in, any account.
//! - [`MaybeUser`]: signed in or not, for endpoints that serve both.
//!
//! There used to be a third for administrators. Roles replaced it: what a
//! handler needs is a *permission*, and an extractor for "holds `users:write`"
//! would be one type per permission. So the extractor answers who is asking and
//! the handler names what it requires, with [`CurrentUser::require`] or
//! `admitted` for the administrative surface.
//!
//! A handler cannot forget to ask who is calling: there is no way to reach the
//! account except through one of these. It *can* forget to check a permission,
//! so every administrative route goes through one helper instead of
//! spelling the check itself.

use std::{convert::Infallible, net::SocketAddr};

use axum::{
    extract::{ConnectInfo, FromRequestParts},
    http::{header, request::Parts},
};
use tessera_core::{Permission, PermissionSet, SessionId};
use time::OffsetDateTime;

use crate::{
    auth::{cookie, proxy::Origin, secret::Secret},
    db::repository::user::User,
    error::Error,
    state::AppState,
};

/// How a request proved who it was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Credential {
    /// A browser session cookie.
    Session {
        /// Which session it was.
        ///
        /// Carried, not looked up again, because two things need it and
        /// would otherwise find the row a second time: ending every session
        /// *except this one*, and marking the current entry in a list. A
        /// handler that re-finds its own session by secret can get it wrong.
        id: SessionId,
        /// When it signed in. Not refreshed by use, so it says how long ago the
        /// user last proved who they are; see [`Credential::signed_in_since`].
        signed_in_at: OffsetDateTime,
    },
    /// A bearer token, from a script, the CLI or a bot.
    ApiToken,
}

impl Credential {
    /// Whether a browser session proved this request.
    ///
    /// Four handlers ask this, all for the same reason: a leaked API token must
    /// not be able to mint another token, revoke one, change the password, or
    /// sign the account out. Keeping it a question, not an equality
    /// check means adding a field to [`Credential::Session`] touches none of
    /// them.
    #[must_use]
    pub const fn is_session(self) -> bool {
        matches!(self, Self::Session { .. })
    }

    /// The session this request came from, if it came from one.
    #[must_use]
    pub const fn session_id(self) -> Option<SessionId> {
        match self {
            Self::Session { id, .. } => Some(id),
            Self::ApiToken => None,
        }
    }

    /// Whether this is a session that signed in at or after `since`.
    ///
    /// How an account with no password confirms a sensitive change: by having
    /// just signed in at its provider instead of typing something. A session
    /// that was merely used recently does not count; only the sign-in itself,
    /// the one time the provider was asked.
    #[must_use]
    pub fn signed_in_since(self, since: OffsetDateTime) -> bool {
        matches!(self, Self::Session { signed_in_at, .. } if signed_in_at >= since)
    }
}

/// An authenticated account that is free to act.
///
/// An account whose password must be changed is **not** one of these. The check
/// lives here, in the extractor every handler already goes through, instead of
/// in each handler: a flag the server reports but does not enforce is a request
/// that the client please behave, and the one account carrying it is the
/// instance's administrator.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// The account.
    pub user: User,
    /// Which mechanism proved it, so a handler can refuse one where that
    /// matters, such as changing a password with an API token.
    pub credential: Credential,
    /// Everything this account is allowed to do, from every role it is in.
    ///
    /// Loaded once here, not queried by each handler. A handler that has
    /// to remember to ask is a handler that can forget to.
    pub permissions: PermissionSet,
}

impl CurrentUser {
    /// Refuses the request unless this account holds the permission.
    ///
    /// # Why the handler names it instead of an extractor
    ///
    /// `AdminUser` said "one of the privileged endpoints" and nothing more, so
    /// every one was as powerful as the most powerful. Naming the permission at
    /// the point of use is what makes a role like "can invite people but cannot
    /// delete accounts" possible, and it is greppable: "what does
    /// `users:delete` protect" is a search.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Forbidden`] when the account does not hold it. Forbidden
    /// and not not-found, because the caller is authenticated and the
    /// contract already says the endpoint exists.
    pub fn require(&self, permission: Permission) -> Result<(), Error> {
        if self.permissions.allows(permission) {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
}

/// An authenticated account, whether or not it owes a password change.
///
/// For the three endpoints that must keep working while an account is held in
/// the change-password flow: reading its own identity, changing the password,
/// and signing out. Everything else takes [`CurrentUser`], so forgetting this
/// state gives the strict behaviour, not the lax one.
#[derive(Debug, Clone)]
pub struct SelfServiceUser(pub CurrentUser);

/// An account, if the request carried one.
#[derive(Debug, Clone)]
pub struct MaybeUser(pub Option<CurrentUser>);

impl FromRequestParts<AppState> for MaybeUser {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // An instance with accounts turned off has no account to be. Answering
        // "not signed in" instead of erroring keeps every endpoint that serves
        // both cases working unchanged.
        if !state.config().features.accounts {
            return Ok(Self(None));
        }
        Ok(Self(authenticate(parts, state).await?))
    }
}

impl FromRequestParts<AppState> for SelfServiceUser {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let MaybeUser(user) = MaybeUser::from_request_parts(parts, state).await?;
        user.map(Self).ok_or(Error::Unauthorized)
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = Error;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let SelfServiceUser(current) = SelfServiceUser::from_request_parts(parts, state).await?;
        if current.user.must_change_password {
            return Err(Error::PasswordChangeRequired);
        }
        Ok(current)
    }
}

/// Where a request appears to have come from.
///
/// Used for throttle keys, never for authorisation: it is a hint, and a shared
/// NAT or a VPN makes many people look like one address.
///
/// Carries whether it came through a proxy this instance trusts, because that
/// one fact decides both which address to count against and whether
/// `X-Forwarded-Proto` is worth reading. Working it out twice is how the two
/// answers drift apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientAddress(pub Origin);

impl FromRequestParts<AppState> for ClientAddress {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // The peer address is only present when the server was started with
        // `into_make_service_with_connect_info`. A test harness that calls the
        // router directly has none, so this cannot fail.
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| address.ip());
        Ok(Self(state.proxies().resolve(peer, &parts.headers)))
    }
}

/// Resolves whichever credential the request carried.
///
/// The cookie is tried first, because a browser sends it on every request while
/// a bearer token appears only when something attached one. A request carrying
/// both is a browser calling with an explicit token, and the
/// explicit one should not be silently ignored, so the header wins.
async fn authenticate(parts: &Parts, state: &AppState) -> Result<Option<CurrentUser>, Error> {
    if let Some(secret) = bearer_token(parts) {
        return authenticate_token(&secret, state).await;
    }
    if let Some(secret) = cookie::session_secret(&parts.headers) {
        return authenticate_session(&secret, state).await;
    }
    Ok(None)
}

fn bearer_token(parts: &Parts) -> Option<Secret> {
    parts
        .headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            let (scheme, token) = value.split_once(' ')?;
            scheme
                .eq_ignore_ascii_case("bearer")
                .then(|| Secret::from_client(token.trim()))
        })
}

async fn authenticate_session(
    secret: &Secret,
    state: &AppState,
) -> Result<Option<CurrentUser>, Error> {
    let Some(session) = state.sessions.find_by_secret(secret).await? else {
        return Ok(None);
    };

    let now = OffsetDateTime::now_utc();
    if !session.is_valid_at(now) {
        // Refused, and the row is left alone. It used to be deleted here, on
        // the grounds that a presented-but-dead session was the cheapest moment
        // to clean one up, but the row is the record of that login now and this
        // is when it matters most. A revoked session being presented means the
        // credential the owner took away is still in use, and deleting it would
        // erase the entry they need to see.
        return Ok(None);
    }

    let Some(user) = state.users.find_by_id(session.user_id).await? else {
        // The account is gone but the session was not. The foreign key should
        // make this impossible; if it happens, the session is worthless.
        state.sessions.delete(session.id).await?;
        return Ok(None);
    };
    // Suspending or removing an account has to stop the sessions it already
    // holds, not only the next sign-in, or an account suspended at noon keeps
    // working until its cookie happens to expire.
    if !user.state.may_sign_in() {
        return Ok(None);
    }

    state.sessions.touch(session.id, now).await?;
    let permissions = state.roles.permissions_of(user.id).await?;
    Ok(Some(CurrentUser {
        user,
        credential: Credential::Session {
            id: session.id,
            signed_in_at: session.created_at,
        },
        permissions,
    }))
}

async fn authenticate_token(
    secret: &Secret,
    state: &AppState,
) -> Result<Option<CurrentUser>, Error> {
    let Some(token) = state.tokens.find_by_secret(secret).await? else {
        return Ok(None);
    };

    let now = OffsetDateTime::now_utc();
    if !token.is_usable_at(now) {
        // Revoked and expired both land here, and both look identical to the
        // caller. Distinguishing them would tell someone holding a stolen token
        // whether it was noticed.
        return Ok(None);
    }

    let Some(user) = state.users.find_by_id(token.user_id).await? else {
        return Ok(None);
    };
    // Same for a token: it acts as its owner, and an owner who may not sign in
    // may not act.
    if !user.state.may_sign_in() {
        return Ok(None);
    }

    state.tokens.touch(token.id, now).await?;
    // What the token was given, and no more than its owner may do *now*. Both
    // halves matter: a token cannot exceed the account that minted it, and
    // demoting an account narrows every token it holds at that moment, not
    // whenever the tokens are next looked at.
    let owner = state.roles.permissions_of(user.id).await?;
    let permissions: PermissionSet = token
        .permissions
        .iter()
        .filter(|permission| owner.allows(*permission))
        .collect();
    Ok(Some(CurrentUser {
        user,
        credential: Credential::ApiToken,
        permissions,
    }))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderValue, Request};

    use super::*;

    fn parts_with(header_name: &'static str, value: &str) -> Parts {
        let mut request = Request::new(());
        request.headers_mut().insert(
            header_name,
            HeaderValue::from_str(value).expect("valid header"),
        );
        request.into_parts().0
    }

    #[test]
    fn reads_a_bearer_token() {
        let parts = parts_with("authorization", "Bearer tsk_abc123");
        assert_eq!(
            bearer_token(&parts).map(|s| s.expose().to_owned()),
            Some("tsk_abc123".to_owned())
        );
    }

    /// RFC 7235 says the scheme is case-insensitive, and clients disagree about
    /// which case to send.
    #[test]
    fn the_bearer_scheme_is_case_insensitive() {
        for value in ["bearer x", "BEARER x", "BeArEr x"] {
            assert!(
                bearer_token(&parts_with("authorization", value)).is_some(),
                "{value}"
            );
        }
    }

    #[test]
    fn ignores_other_authorization_schemes() {
        for value in ["Basic dXNlcjpwYXNz", "Digest something", "tsk_no_scheme"] {
            assert!(
                bearer_token(&parts_with("authorization", value)).is_none(),
                "{value}"
            );
        }
    }

    #[test]
    fn no_authorization_header_means_no_token() {
        let parts = Request::new(()).into_parts().0;
        assert!(bearer_token(&parts).is_none());
    }
}
