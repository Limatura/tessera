//! Turning a request into the account that made it.
//!
//! Three extractors, so a handler's signature states what it needs and the
//! compiler enforces it:
//!
//! - [`CurrentUser`] — signed in, any account.
//! - [`AdminUser`] — signed in and an administrator.
//! - [`MaybeUser`] — signed in or not; for endpoints that serve both.
//!
//! A handler that forgets to check is not possible: there is no way to reach the
//! account except by asking for it, and asking for [`AdminUser`] is the only way
//! to get one.

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
        /// Carried rather than looked up again, because two things need it and
        /// both would otherwise have to find the row a second time: ending every
        /// session *except this one*, and marking the current entry in a list of
        /// them. A handler that has to re-find its own session by secret is one
        /// that can get the answer wrong.
        id: SessionId,
    },
    /// A bearer token, from a script, the CLI or a bot.
    ApiToken,
}

impl Credential {
    /// Whether a browser session proved this request.
    ///
    /// Four handlers ask this, and they all ask it for the same reason: a leaked
    /// API token must not be able to mint another token, revoke one, change the
    /// password, or sign the account out. Keeping it a question rather than an
    /// equality check means adding a field to [`Credential::Session`] does not
    /// touch any of them.
    #[must_use]
    pub const fn is_session(self) -> bool {
        matches!(self, Self::Session { .. })
    }

    /// The session this request came from, if it came from one.
    #[must_use]
    pub const fn session_id(self) -> Option<SessionId> {
        match self {
            Self::Session { id } => Some(id),
            Self::ApiToken => None,
        }
    }
}

/// An authenticated account that is free to act.
///
/// An account whose password must be changed is **not** one of these. That check
/// lives here, in the extractor every handler already has to go through, rather
/// than in each handler — a flag the server reports but does not enforce is a
/// request that the client please behave, and the one account that carries it is
/// the administrator of the instance.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    /// The account.
    pub user: User,
    /// Which mechanism proved it, so a handler can refuse one of them where
    /// that matters — changing a password with an API token, for instance.
    pub credential: Credential,
    /// Everything this account is allowed to do, from every role it is in.
    ///
    /// Loaded once here rather than queried by each handler: a handler that has
    /// to remember to ask is a handler that can forget to.
    pub permissions: PermissionSet,
}

impl CurrentUser {
    /// Refuses the request unless this account holds the permission.
    ///
    /// # Why the handler names it rather than an extractor
    ///
    /// `AdminUser` said "one of the privileged endpoints" and nothing more, so
    /// every one of them was as powerful as the most powerful. Naming the
    /// permission at the point of use is what makes a role like "can invite
    /// people but cannot delete accounts" possible, and it is greppable: the
    /// answer to "what does `users:delete` protect" is a search.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Forbidden`] when the account does not hold it. Forbidden
    /// rather than not-found: the caller is authenticated, so telling them the
    /// endpoint exists reveals nothing they could not read in the contract.
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
/// and signing out. Everything else takes [`CurrentUser`], so forgetting to
/// think about this state gives you the strict behaviour rather than the lax one.
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
        // An instance with accounts turned off has nobody to be. Answering
        // "not signed in" rather than erroring keeps every endpoint that serves
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
/// Carries whether it came through a proxy this instance trusts, because the
/// same fact decides two things — which address to count against, and whether
/// `X-Forwarded-Proto` is worth reading — and working it out twice is how the
/// two answers drift apart.
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
        // router directly has none, which is why this cannot fail.
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ConnectInfo(address)| address.ip());
        Ok(Self(state.proxies().resolve(peer, &parts.headers)))
    }
}

/// Resolves whichever credential the request carried.
///
/// The cookie is tried first because a browser sends it on every request,
/// whereas a bearer token is only present when something deliberately attached
/// one. A request carrying both is a browser calling with an explicit token, and
/// the explicit one should not be silently ignored — so the header wins.
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
        // Refused, and the row is left alone. It used to be deleted here, on the
        // grounds that a presented-but-dead session was the cheapest moment to
        // clean one up — but the row is the record of that login now, and this
        // is the moment it matters most: somebody presenting a session that was
        // revoked is somebody still holding a credential the owner took away.
        // Deleting it would erase exactly the entry they need to see.
        return Ok(None);
    }

    let Some(user) = state.users.find_by_id(session.user_id).await? else {
        // The account is gone but the session was not. The foreign key should
        // make this impossible; if it happens, the session is worthless.
        state.sessions.delete(session.id).await?;
        return Ok(None);
    };
    // Suspending or removing an account has to stop the sessions it already
    // holds, not only the next sign-in — otherwise somebody suspended at noon
    // carries on working until their cookie happens to expire.
    if !user.state.may_sign_in() {
        return Ok(None);
    }

    state.sessions.touch(session.id, now).await?;
    let permissions = state.roles.permissions_of(user.id).await?;
    Ok(Some(CurrentUser {
        user,
        credential: Credential::Session { id: session.id },
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
    // demoting somebody narrows every token they hold at that moment rather
    // than at the next time somebody remembers to look.
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
