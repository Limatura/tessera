//! The audit log: who did what to which thing, and whether it worked.
//!
//! Not the server's log (`crate::log_files`): that is the process describing
//! itself, this is a table of things people did, filterable by person and by
//! the thing they touched.
//!
//! # What is recorded
//!
//! - **Shares**: created, retrieved (including a wrong code), downloaded, removed by the owner or
//!   by the sweep. Can be switched off with `audit.record_share_events`.
//! - **Sign-in**: every sign-in, failed or not, and every sign-out.
//! - **Administration**: every change an administrator makes, and every read of this log's export.
//!
//! Sign-in and administration cannot be switched off. An audit log that the
//! people it audits can turn off answers nothing.
//!
//! # Whose address is kept
//!
//! The address and browser are kept for everything an account does, and for
//! every sign-in attempt. For someone with no account fetching a share, they
//! are only kept while `share.access_log.enabled` is on: that switch already
//! decides whether visitors are recorded at all, and this log must not become
//! a way round it.
//!
//! # What is never recorded
//!
//! A secret. A sealed setting is written as `"changed"`, a token by its id and
//! never its value, and a failed sign-in never carries what was typed into the
//! name field, so the log cannot be used to learn which accounts exist.
//!
//! # A failed write is not the caller's problem
//!
//! The thing being recorded has already happened. Refusing it because a row
//! would not insert turns record-keeping into an outage, so a failure is
//! logged as a warning and goes no further. Retrievals and downloads, the
//! frequent ones, are written in the background so they never wait on it.

use std::convert::Infallible;

use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, header, request::Parts},
};
use serde_json::Value;
use tessera_core::{ApiTokenId, InviteId, RoleId, ShareId, UserId};
use time::OffsetDateTime;

use crate::{
    auth::{
        extract::{ClientAddress, Credential, CurrentUser},
        proxy::UNPLACEABLE,
    },
    db::repository::audit_event::{ActorKind, NewAuditEvent, Outcome},
    state::AppState,
};

/// Every action this server records, as stored.
pub mod action {
    /// A share was created.
    pub const SHARE_CREATE: &str = "share.create";
    /// A share was asked for by code: a retrieval was spent, or the code was
    /// wrong.
    pub const SHARE_RETRIEVE: &str = "share.retrieve";
    /// A share's content was handed over.
    pub const SHARE_DOWNLOAD: &str = "share.download";
    /// A share was removed by its owner, or by the sweep once it was finished.
    pub const SHARE_DELETE: &str = "share.delete";
    /// An administrator removed somebody's share.
    pub const SHARE_ADMIN_DELETE: &str = "share.admin_delete";
    /// Someone signed in, or tried to.
    pub const AUTH_LOGIN: &str = "auth.login";
    /// Someone signed out.
    pub const AUTH_LOGOUT: &str = "auth.logout";
    /// A setting was changed from the panel.
    pub const SETTINGS_UPDATE: &str = "settings.update";
    /// An account was changed by an administrator.
    pub const USER_UPDATE: &str = "user.update";
    /// An account was suspended.
    pub const USER_DISABLE: &str = "user.disable";
    /// An account was removed.
    pub const USER_DELETE: &str = "user.delete";
    /// An account was given a different role.
    pub const USER_ROLE_CHANGE: &str = "user.role_change";
    /// A role was made.
    pub const ROLE_CREATE: &str = "role.create";
    /// A role was changed.
    pub const ROLE_UPDATE: &str = "role.update";
    /// A role was removed.
    pub const ROLE_DELETE: &str = "role.delete";
    /// An invite code was minted.
    pub const INVITE_CREATE: &str = "invite.create";
    /// An invite code was revoked.
    pub const INVITE_REVOKE: &str = "invite.revoke";
    /// An API token was minted.
    pub const API_TOKEN_CREATE: &str = "api_token.create";
    /// An API token was revoked.
    pub const API_TOKEN_REVOKE: &str = "api_token.revoke";
    /// This log was exported.
    pub const AUDIT_EXPORT: &str = "audit.export";

    /// Every one, for a filter to check against and a screen to offer.
    pub const ALL: &[&str] = &[
        SHARE_CREATE,
        SHARE_RETRIEVE,
        SHARE_DOWNLOAD,
        SHARE_DELETE,
        SHARE_ADMIN_DELETE,
        AUTH_LOGIN,
        AUTH_LOGOUT,
        SETTINGS_UPDATE,
        USER_UPDATE,
        USER_DISABLE,
        USER_DELETE,
        USER_ROLE_CHANGE,
        ROLE_CREATE,
        ROLE_UPDATE,
        ROLE_DELETE,
        INVITE_CREATE,
        INVITE_REVOKE,
        API_TOKEN_CREATE,
        API_TOKEN_REVOKE,
        AUDIT_EXPORT,
    ];

    /// The ones `audit.record_share_events` switches off. Not
    /// [`SHARE_ADMIN_DELETE`]: that is an administrator's action, and those
    /// are always recorded.
    pub(super) const SHARE_EVENTS: &[&str] =
        &[SHARE_CREATE, SHARE_RETRIEVE, SHARE_DOWNLOAD, SHARE_DELETE];
}

/// Who did it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor {
    kind: ActorKind,
    user_id: Option<UserId>,
    api_token_id: Option<ApiTokenId>,
}

impl Actor {
    /// The account behind this request, and the token when it used one.
    #[must_use]
    pub const fn of(current: &CurrentUser) -> Self {
        match current.credential {
            Credential::Session { .. } => Self::user(current.user.id),
            Credential::ApiToken { id } => Self {
                kind: ActorKind::ApiToken,
                user_id: Some(current.user.id),
                api_token_id: Some(id),
            },
        }
    }

    /// The account behind this request if there was one, anonymous if not.
    #[must_use]
    pub const fn maybe(current: Option<&CurrentUser>) -> Self {
        match current {
            Some(current) => Self::of(current),
            None => Self::anonymous(),
        }
    }

    /// An account, known by its id: someone who has just proved who they are.
    #[must_use]
    pub const fn user(id: UserId) -> Self {
        Self {
            kind: ActorKind::User,
            user_id: Some(id),
            api_token_id: None,
        }
    }

    /// Nobody signed in.
    #[must_use]
    pub const fn anonymous() -> Self {
        Self {
            kind: ActorKind::Anonymous,
            user_id: None,
            api_token_id: None,
        }
    }

    /// The instance itself.
    #[must_use]
    pub const fn system() -> Self {
        Self {
            kind: ActorKind::System,
            user_id: None,
            api_token_id: None,
        }
    }
}

/// What it was done to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// A share.
    Share(ShareId),
    /// An account.
    User(UserId),
    /// A role.
    Role(RoleId),
    /// An invite code.
    Invite(InviteId),
    /// A setting, by its dotted key.
    Setting(String),
    /// An API token.
    ApiToken(ApiTokenId),
}

impl Target {
    fn split(self) -> (&'static str, String) {
        match self {
            Self::Share(id) => ("share", id.to_string()),
            Self::User(id) => ("user", id.to_string()),
            Self::Role(id) => ("role", id.to_string()),
            Self::Invite(id) => ("invite", id.to_string()),
            Self::Setting(key) => ("setting", key),
            Self::ApiToken(id) => ("api_token", id.to_string()),
        }
    }
}

/// Every target type, for a filter to check against.
pub const TARGET_TYPES: &[&str] = &["share", "user", "role", "invite", "setting", "api_token"];

/// Where a request came from, as far as it says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Request {
    ip_address: Option<String>,
    user_agent: Option<String>,
}

impl Request {
    /// From the throttle's idea of the source and the request's headers.
    ///
    /// The address follows the rule `access_events` does: the throttle's
    /// stand-in for "there was none" is not stored as one.
    #[must_use]
    pub fn new(source: &str, headers: &HeaderMap) -> Self {
        Self {
            ip_address: (source != UNPLACEABLE).then(|| source.to_owned()),
            user_agent: headers
                .get(header::USER_AGENT)
                .and_then(|value| value.to_str().ok())
                .map(ToOwned::to_owned),
        }
    }

    /// No request at all, for what the instance does by itself.
    #[must_use]
    pub fn none() -> Self {
        Self::default()
    }
}

impl FromRequestParts<AppState> for Request {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let ClientAddress(origin) = ClientAddress::from_request_parts(parts, state).await?;
        Ok(Self::new(&origin.source, &parts.headers))
    }
}

/// One thing to record.
#[derive(Debug, Clone)]
#[must_use = "an event does nothing until it is recorded"]
pub struct Event {
    action: &'static str,
    outcome: Outcome,
    actor: Actor,
    target: Option<Target>,
    metadata: Option<Value>,
}

impl Event {
    /// Something that worked.
    pub const fn success(action: &'static str, actor: Actor) -> Self {
        Self {
            action,
            outcome: Outcome::Success,
            actor,
            target: None,
            metadata: None,
        }
    }

    /// Something that was refused or failed.
    pub const fn failure(action: &'static str, actor: Actor) -> Self {
        Self {
            action,
            outcome: Outcome::Failure,
            actor,
            target: None,
            metadata: None,
        }
    }

    /// What it was done to.
    pub fn on(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    /// What else is worth knowing about it. Never a secret.
    pub fn with(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// The row this becomes, or `None` when this instance does not record it.
    fn into_row(self, state: &AppState, request: &Request) -> Option<NewAuditEvent> {
        let (record_shares, record_visitors) = {
            let config = state.config();
            (
                config.audit.record_share_events,
                config.share.access_log.enabled,
            )
        };
        if !record_shares && action::SHARE_EVENTS.contains(&self.action) {
            return None;
        }
        // A visitor to a share is only placed while the access log is on. See
        // the module docs.
        let placed = record_visitors
            || self.actor.kind != ActorKind::Anonymous
            || !action::SHARE_EVENTS.contains(&self.action);
        let (target_type, target_id) = self.target.map(Target::split).unzip();
        Some(NewAuditEvent {
            occurred_at: OffsetDateTime::now_utc(),
            action: self.action,
            outcome: self.outcome,
            actor_kind: self.actor.kind,
            actor_user_id: self.actor.user_id,
            api_token_id: self.actor.api_token_id,
            target_type,
            target_id,
            ip_address: request.ip_address.clone().filter(|_| placed),
            user_agent: request.user_agent.clone().filter(|_| placed),
            metadata: self.metadata,
        })
    }

    /// Writes it, and waits for the write.
    ///
    /// For everything that is not on a download's path: an administrator's
    /// change is rare, and waiting means the entry is there by the time the
    /// response is.
    pub async fn record(self, state: &AppState, request: &Request) {
        let action = self.action;
        let Some(row) = self.into_row(state, request) else {
            return;
        };
        if let Err(error) = state.audit.record(row).await {
            tracing::warn!(?error, action, "could not write an audit entry");
        }
    }

    /// Writes it in the background, for the frequent ones a download must not
    /// wait on.
    pub fn record_detached(self, state: &AppState, request: &Request) {
        let action = self.action;
        let Some(row) = self.into_row(state, request) else {
            return;
        };
        let repository = state.audit.clone();
        tokio::spawn(async move {
            if let Err(error) = repository.record(row).await {
                tracing::warn!(?error, action, "could not write an audit entry");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{TARGET_TYPES, Target, action};

    #[test]
    fn every_share_event_is_an_action() {
        for name in action::SHARE_EVENTS {
            assert!(action::ALL.contains(name), "{name}");
        }
        assert!(!action::SHARE_EVENTS.contains(&action::SHARE_ADMIN_DELETE));
    }

    #[test]
    fn every_target_splits_into_a_known_type() {
        let (kind, id) = Target::Setting("share.code_length".to_owned()).split();
        assert!(TARGET_TYPES.contains(&kind));
        assert_eq!(id, "share.code_length");
    }

    #[test]
    fn actions_are_namespaced() {
        for name in action::ALL {
            assert!(name.contains('.'), "{name}");
        }
    }
}
