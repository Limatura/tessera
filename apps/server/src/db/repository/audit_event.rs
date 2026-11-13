//! Writing, listing and sweeping the audit log.
//!
//! # What this is for
//!
//! "Who changed this setting", "who deleted that share", "has anyone been
//! trying passwords on this account". The server's own log could answer some of
//! that, but it is the process talking about itself, it rotates on a schedule
//! set for disk space, and it cannot be filtered by the thing that was touched.
//! This is a table of events, one row per thing done.
//!
//! See `crate::audit` for what is recorded and when. This module only knows the
//! columns.

use sea_orm::{
    ActiveValue::Set, ColumnTrait as _, Condition, DatabaseConnection, EntityTrait as _,
    PaginatorTrait as _, QueryFilter as _, QueryOrder as _, QuerySelect as _,
};
use tessera_core::{ApiTokenId, AuditEventId, UserId};
use time::OffsetDateTime;

use crate::db::entity::audit_event::{self, Entity as AuditEventEntity};

/// Whether what was attempted happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// It did.
    Success,
    /// It was refused or it failed.
    Failure,
}

impl Outcome {
    /// The spelling stored and sent.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Failure => "failure",
        }
    }

    /// Reads one back.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            _ => None,
        }
    }
}

/// Who did it, by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    /// Nobody signed in: someone holding a link, or someone trying to sign in.
    Anonymous,
    /// An account, through a browser session.
    User,
    /// An account, through one of its API tokens.
    ApiToken,
    /// The instance itself, such as the sweep removing an expired share.
    System,
}

impl ActorKind {
    /// The spelling stored and sent.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Anonymous => "anonymous",
            Self::User => "user",
            Self::ApiToken => "api_token",
            Self::System => "system",
        }
    }

    /// Reads one back. Anything unknown is the instance, the least misleading
    /// reading of a row a later version wrote.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        match value {
            "anonymous" => Self::Anonymous,
            "user" => Self::User,
            "api_token" => Self::ApiToken,
            _ => Self::System,
        }
    }
}

/// One entry, before it is written.
#[derive(Debug, Clone)]
pub struct NewAuditEvent {
    /// When it happened.
    pub occurred_at: OffsetDateTime,
    /// What was done, such as `share.create`.
    pub action: &'static str,
    /// Whether it worked.
    pub outcome: Outcome,
    /// Who did it, by kind.
    pub actor_kind: ActorKind,
    /// The account, when there was one.
    pub actor_user_id: Option<UserId>,
    /// The token, when one was used.
    pub api_token_id: Option<ApiTokenId>,
    /// What kind of thing it was done to.
    pub target_type: Option<&'static str>,
    /// Which one.
    pub target_id: Option<String>,
    /// Where it came from, when there is an honest answer.
    pub ip_address: Option<String>,
    /// What the client called itself.
    pub user_agent: Option<String>,
    /// Whatever this action records. Never a secret.
    pub metadata: Option<serde_json::Value>,
}

/// One entry, as read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEvent {
    /// Its identifier, which is also the cursor for the page after it.
    pub id: String,
    /// When it happened.
    pub occurred_at: OffsetDateTime,
    /// What was done.
    pub action: String,
    /// `success` or `failure`.
    pub outcome: String,
    /// Who did it, by kind.
    pub actor_kind: ActorKind,
    /// The account, when there was one.
    pub actor_user_id: Option<String>,
    /// The token, when one was used.
    pub api_token_id: Option<String>,
    /// What kind of thing it was done to.
    pub target_type: Option<String>,
    /// Which one.
    pub target_id: Option<String>,
    /// Where it came from.
    pub ip_address: Option<String>,
    /// What the client called itself.
    pub user_agent: Option<String>,
    /// Whatever this action recorded.
    pub metadata: Option<serde_json::Value>,
}

/// Which actions a listing wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionPattern {
    /// Exactly this one, such as `share.create`.
    Exact(String),
    /// Everything starting with this, such as `auth.` for `auth.*`.
    Prefix(String),
}

/// What a listing narrows to. Every field left empty matches everything.
#[derive(Debug, Clone, Default)]
pub struct AuditFilter {
    /// Any of these actions. Empty is every action.
    pub actions: Vec<ActionPattern>,
    /// Only this outcome.
    pub outcome: Option<Outcome>,
    /// Only what this account did.
    pub actor_user_id: Option<UserId>,
    /// Only what was done to this kind of thing.
    pub target_type: Option<String>,
    /// Only what was done to this one.
    pub target_id: Option<String>,
    /// Nothing before this.
    pub from: Option<OffsetDateTime>,
    /// Nothing after this.
    pub to: Option<OffsetDateTime>,
}

impl AuditFilter {
    fn condition(&self) -> Condition {
        let mut all = Condition::all();
        if !self.actions.is_empty() {
            let mut any = Condition::any();
            for pattern in &self.actions {
                any = any.add(match pattern {
                    ActionPattern::Exact(action) => audit_event::Column::Action.eq(action.as_str()),
                    ActionPattern::Prefix(prefix) => {
                        audit_event::Column::Action.starts_with(prefix.as_str())
                    }
                });
            }
            all = all.add(any);
        }
        if let Some(outcome) = self.outcome {
            all = all.add(audit_event::Column::Outcome.eq(outcome.as_str()));
        }
        if let Some(actor) = self.actor_user_id {
            all = all.add(audit_event::Column::ActorUserId.eq(actor.to_string()));
        }
        if let Some(kind) = &self.target_type {
            all = all.add(audit_event::Column::TargetType.eq(kind.as_str()));
        }
        if let Some(id) = &self.target_id {
            all = all.add(audit_event::Column::TargetId.eq(id.as_str()));
        }
        if let Some(from) = self.from {
            all = all.add(audit_event::Column::OccurredAt.gte(from));
        }
        if let Some(to) = self.to {
            all = all.add(audit_event::Column::OccurredAt.lte(to));
        }
        all
    }
}

/// The longest User-Agent kept, for the reason `access_event` gives.
const MAX_USER_AGENT: usize = 512;

/// The audit log, as the rest of the server sees it.
#[derive(Debug, Clone)]
pub struct AuditEventRepository {
    connection: DatabaseConnection,
}

impl AuditEventRepository {
    /// Wraps a connection.
    #[must_use]
    pub const fn new(connection: DatabaseConnection) -> Self {
        Self { connection }
    }

    /// Writes one entry.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the insert fails.
    pub async fn record(&self, event: NewAuditEvent) -> Result<(), sea_orm::DbErr> {
        AuditEventEntity::insert(audit_event::ActiveModel {
            id: Set(AuditEventId::generate().to_string()),
            occurred_at: Set(event.occurred_at),
            action: Set(event.action.to_owned()),
            outcome: Set(event.outcome.as_str().to_owned()),
            actor_kind: Set(event.actor_kind.as_str().to_owned()),
            actor_user_id: Set(event.actor_user_id.map(|id| id.to_string())),
            api_token_id: Set(event.api_token_id.map(|id| id.to_string())),
            target_type: Set(event.target_type.map(ToOwned::to_owned)),
            target_id: Set(event.target_id),
            ip_address: Set(event.ip_address),
            user_agent: Set(event
                .user_agent
                .map(|agent| truncate(&agent, MAX_USER_AGENT))),
            metadata: Set(event.metadata.map(|value| value.to_string())),
        })
        .exec(&self.connection)
        .await?;
        Ok(())
    }

    /// A page of entries matching `filter`, newest first, starting after
    /// `cursor` (the last id of the previous page).
    ///
    /// Ordered by id, not by time: a ULID sorts in the order it was made, and
    /// a cursor on a unique column cannot skip or repeat two rows written in
    /// the same instant.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the query fails.
    pub async fn list(
        &self,
        filter: &AuditFilter,
        cursor: Option<&str>,
        limit: u64,
    ) -> Result<Vec<AuditEvent>, sea_orm::DbErr> {
        let mut query = AuditEventEntity::find().filter(filter.condition());
        if let Some(cursor) = cursor {
            query = query.filter(audit_event::Column::Id.lt(cursor));
        }
        Ok(query
            .order_by_desc(audit_event::Column::Id)
            .limit(limit)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| AuditEvent {
                id: row.id,
                occurred_at: row.occurred_at,
                action: row.action,
                outcome: row.outcome,
                actor_kind: ActorKind::parse(&row.actor_kind),
                actor_user_id: row.actor_user_id,
                api_token_id: row.api_token_id,
                target_type: row.target_type,
                target_id: row.target_id,
                ip_address: row.ip_address,
                user_agent: row.user_agent,
                metadata: row
                    .metadata
                    .and_then(|text| serde_json::from_str(&text).ok()),
            })
            .collect())
    }

    /// How many entries a cutoff would remove.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the query fails.
    pub async fn count_before(&self, cutoff: OffsetDateTime) -> Result<u64, sea_orm::DbErr> {
        AuditEventEntity::find()
            .filter(audit_event::Column::OccurredAt.lt(cutoff))
            .count(&self.connection)
            .await
    }

    /// Removes entries older than `cutoff`.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the delete fails.
    pub async fn delete_before(&self, cutoff: OffsetDateTime) -> Result<u64, sea_orm::DbErr> {
        Ok(AuditEventEntity::delete_many()
            .filter(audit_event::Column::OccurredAt.lt(cutoff))
            .exec(&self.connection)
            .await?
            .rows_affected)
    }
}

/// Cuts a string to `limit` bytes without splitting a character.
fn truncate(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_owned();
    }
    let mut end = limit;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.get(..end).unwrap_or_default().to_owned()
}

#[cfg(test)]
mod tests {
    use super::{ActorKind, Outcome, truncate};

    #[test]
    fn outcomes_round_trip() {
        for outcome in [Outcome::Success, Outcome::Failure] {
            assert_eq!(Outcome::parse(outcome.as_str()), Some(outcome));
        }
        assert_eq!(Outcome::parse("maybe"), None);
    }

    #[test]
    fn actor_kinds_round_trip() {
        for kind in [
            ActorKind::Anonymous,
            ActorKind::User,
            ActorKind::ApiToken,
            ActorKind::System,
        ] {
            assert_eq!(ActorKind::parse(kind.as_str()), kind);
        }
    }

    #[test]
    fn cutting_never_splits_a_character() {
        assert_eq!(truncate("中文中文", 4), "中");
    }
}
