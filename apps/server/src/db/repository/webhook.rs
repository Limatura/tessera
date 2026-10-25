//! Webhooks, and the events on their way to them.
//!
//! One repository for both tables, because they are one feature: an event is
//! queued for a webhook, and removing the webhook takes its queue with it.

use std::collections::HashMap;

use sea_orm::{
    ActiveValue::Set,
    ColumnTrait as _, Condition, DatabaseConnection, EntityTrait as _, PaginatorTrait as _,
    QueryFilter as _, QueryOrder as _, QuerySelect as _,
    sea_query::{Expr, ExprTrait as _},
};
use tessera_core::{UserId, WebhookDeliveryId, WebhookId};
use time::OffsetDateTime;

use crate::{
    db::entity::{webhook, webhook_delivery},
    security::secret::{Sealer, SecretError},
};

/// Anything that can go wrong reading or writing these tables.
#[derive(Debug, thiserror::Error)]
pub enum WebhookRepositoryError {
    /// The database refused the operation.
    #[error(transparent)]
    Database(#[from] sea_orm::DbErr),

    /// The URL or secret could not be sealed or opened.
    #[error(transparent)]
    Secret(#[from] SecretError),

    /// A stored value does not read back.
    #[error("the stored webhook `{0}` is not usable")]
    Corrupt(String),
}

/// Something that happens to a share, which a webhook can be told about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Event {
    /// It was uploaded.
    Created,
    /// A retrieval was counted: a download started, or a note was opened.
    Downloaded,
    /// It expired, or was retrieved as many times as it allowed.
    Expired,
}

impl Event {
    /// Every event, in a stable order.
    pub const ALL: [Self; 3] = [Self::Created, Self::Downloaded, Self::Expired];

    /// The name a webhook is subscribed with and the body carries.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "share.created",
            Self::Downloaded => "share.downloaded",
            Self::Expired => "share.expired",
        }
    }

    /// The event with this name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|event| event.as_str() == name)
    }
}

/// How a webhook's body is shaped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The event as JSON.
    Json,
    /// A message a Discord channel's webhook will post.
    Discord,
}

impl Format {
    /// The stored and accepted spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Discord => "discord",
        }
    }

    /// The format with this spelling.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        [Self::Json, Self::Discord]
            .into_iter()
            .find(|format| format.as_str() == name)
    }
}

/// A webhook, opened.
#[derive(Debug, Clone)]
pub struct Webhook {
    /// Stable identifier.
    pub id: WebhookId,
    /// The account it belongs to, or `None` for the instance's own.
    pub owner: Option<UserId>,
    /// Where events are sent.
    pub url: String,
    /// What it is sent.
    pub events: Vec<Event>,
    /// How the body is shaped.
    pub format: Format,
    /// What the body is signed with.
    pub secret: String,
    /// When it was added.
    pub created_at: OffsetDateTime,
}

/// One event queued for one webhook, as its owner sees it.
#[derive(Debug, Clone)]
pub struct Delivery {
    /// Stable identifier, sent as the `webhook-id` header.
    pub id: WebhookDeliveryId,
    /// Which event.
    pub event: String,
    /// How many times it has been sent.
    pub attempts: u32,
    /// When it is sent next, or `None` once delivered or given up on.
    pub next_attempt_at: Option<OffsetDateTime>,
    /// When the receiver accepted it.
    pub delivered_at: Option<OffsetDateTime>,
    /// The status the receiver last answered with.
    pub last_status: Option<u16>,
    /// Why the last attempt failed.
    pub last_error: Option<String>,
    /// When the event happened.
    pub created_at: OffsetDateTime,
}

/// An event due to be sent, with what sending it needs.
#[derive(Debug, Clone)]
pub struct Due {
    /// The delivery.
    pub id: WebhookDeliveryId,
    /// The webhook it goes to.
    pub webhook: Webhook,
    /// What is sent.
    pub body: String,
    /// How many times it was sent before this one.
    pub attempts: u32,
    /// When the event happened.
    pub created_at: OffsetDateTime,
}

/// How an attempt went.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// The receiver accepted it.
    Delivered {
        /// What it answered.
        status: u16,
    },
    /// It failed and is tried again at `at`, or never when `at` is `None`.
    Failed {
        /// What it answered, if it answered.
        status: Option<u16>,
        /// Why it counts as failed.
        error: String,
        /// When to try again.
        at: Option<OffsetDateTime>,
    },
}

/// The longest error kept for a delivery. A receiver chooses what it answers.
const MAX_ERROR: usize = 512;

/// Webhooks, as the rest of the server sees them.
#[derive(Debug, Clone)]
pub struct WebhookRepository {
    connection: DatabaseConnection,
    sealer: Sealer,
}

impl WebhookRepository {
    /// Wraps a connection, and the key the URLs and secrets are sealed with.
    #[must_use]
    pub const fn new(connection: DatabaseConnection, sealer: Sealer) -> Self {
        Self { connection, sealer }
    }

    /// Adds a webhook.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if sealing or the insert fails.
    pub async fn create(&self, hook: &Webhook) -> Result<(), WebhookRepositoryError> {
        webhook::Entity::insert(webhook::ActiveModel {
            id: Set(hook.id.to_string()),
            user_id: Set(hook.owner.map(|owner| owner.to_string())),
            url: Set(self.sealer.seal(&hook.url)?),
            events: Set(hook
                .events
                .iter()
                .map(|event| event.as_str())
                .collect::<Vec<_>>()
                .join(" ")),
            format: Set(hook.format.as_str().to_owned()),
            secret: Set(self.sealer.seal(&hook.secret)?),
            created_at: Set(hook.created_at),
        })
        .exec(&self.connection)
        .await?;
        Ok(())
    }

    /// The webhooks of one account, or the instance's own for `None`, oldest
    /// first.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if the query fails or a row will not
    /// open.
    pub async fn list(
        &self,
        owner: Option<UserId>,
    ) -> Result<Vec<Webhook>, WebhookRepositoryError> {
        webhook::Entity::find()
            .filter(owned_by(owner))
            .order_by_asc(webhook::Column::Id)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| self.open(&row))
            .collect()
    }

    /// How many webhooks one account, or the instance, has.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if the query fails.
    pub async fn count(&self, owner: Option<UserId>) -> Result<u64, WebhookRepositoryError> {
        Ok(webhook::Entity::find()
            .filter(owned_by(owner))
            .count(&self.connection)
            .await?)
    }

    /// One webhook, if it belongs to `owner`.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if the query fails or the row will not
    /// open.
    pub async fn find(
        &self,
        id: WebhookId,
        owner: Option<UserId>,
    ) -> Result<Option<Webhook>, WebhookRepositoryError> {
        webhook::Entity::find_by_id(id.to_string())
            .filter(owned_by(owner))
            .one(&self.connection)
            .await?
            .map(|row| self.open(&row))
            .transpose()
    }

    /// Removes a webhook belonging to `owner`, and whatever was queued for it.
    /// `false` when there was none.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if a statement fails.
    pub async fn delete(
        &self,
        id: WebhookId,
        owner: Option<UserId>,
    ) -> Result<bool, WebhookRepositoryError> {
        let removed = webhook::Entity::delete_many()
            .filter(webhook::Column::Id.eq(id.to_string()))
            .filter(owned_by(owner))
            .exec(&self.connection)
            .await?;
        Ok(removed.rows_affected > 0)
    }

    /// The webhooks to tell about `event` on a share owned by `owner`: the
    /// instance's, and the owner's own.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if the query fails or a row will not
    /// open.
    pub async fn listening(
        &self,
        event: Event,
        owner: Option<UserId>,
    ) -> Result<Vec<Webhook>, WebhookRepositoryError> {
        let mut whose = Condition::any().add(webhook::Column::UserId.is_null());
        if let Some(owner) = owner {
            whose = whose.add(webhook::Column::UserId.eq(owner.to_string()));
        }
        let rows = webhook::Entity::find()
            .filter(whose)
            .filter(webhook::Column::Events.contains(event.as_str()))
            .all(&self.connection)
            .await?;
        let mut hooks = Vec::with_capacity(rows.len());
        for row in rows {
            let hook = self.open(&row)?;
            if hook.events.contains(&event) {
                hooks.push(hook);
            }
        }
        Ok(hooks)
    }

    /// Queues `body` for a webhook, due now.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if the insert fails.
    pub async fn enqueue(
        &self,
        webhook: WebhookId,
        event: Event,
        body: String,
        now: OffsetDateTime,
    ) -> Result<(), WebhookRepositoryError> {
        webhook_delivery::Entity::insert(webhook_delivery::ActiveModel {
            id: Set(WebhookDeliveryId::generate().to_string()),
            webhook_id: Set(webhook.to_string()),
            event: Set(event.as_str().to_owned()),
            body: Set(body),
            attempts: Set(0),
            next_attempt_at: Set(Some(now)),
            delivered_at: Set(None),
            last_status: Set(None),
            last_error: Set(None),
            created_at: Set(now),
        })
        .exec(&self.connection)
        .await?;
        Ok(())
    }

    /// Up to `limit` events due by `now`, longest waiting first.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if a query fails or a webhook will
    /// not open.
    pub async fn due(
        &self,
        now: OffsetDateTime,
        limit: u64,
    ) -> Result<Vec<Due>, WebhookRepositoryError> {
        let rows = webhook_delivery::Entity::find()
            .filter(webhook_delivery::Column::NextAttemptAt.lte(now))
            .order_by_asc(webhook_delivery::Column::NextAttemptAt)
            .limit(limit)
            .all(&self.connection)
            .await?;
        let mut hooks: HashMap<String, Webhook> = HashMap::new();
        let ids: Vec<String> = rows.iter().map(|row| row.webhook_id.clone()).collect();
        for row in webhook::Entity::find()
            .filter(webhook::Column::Id.is_in(ids))
            .all(&self.connection)
            .await?
        {
            hooks.insert(row.id.clone(), self.open(&row)?);
        }
        let mut due = Vec::with_capacity(rows.len());
        for row in rows {
            // Removed since the query above: its queue went with it.
            let Some(hook) = hooks.get(&row.webhook_id) else {
                continue;
            };
            due.push(Due {
                id: WebhookDeliveryId::parse(&row.id)
                    .map_err(|_| WebhookRepositoryError::Corrupt(row.id.clone()))?,
                webhook: hook.clone(),
                body: row.body,
                attempts: u32::try_from(row.attempts).unwrap_or(0),
                created_at: row.created_at,
            });
        }
        Ok(due)
    }

    /// Records how an attempt went.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if the update fails.
    pub async fn record(
        &self,
        id: WebhookDeliveryId,
        outcome: Outcome,
        now: OffsetDateTime,
    ) -> Result<(), WebhookRepositoryError> {
        let (next, delivered, status, error) = match outcome {
            Outcome::Delivered { status } => (None, Some(now), Some(status), None),
            Outcome::Failed { status, error, at } => (
                at,
                None,
                status,
                Some(error.chars().take(MAX_ERROR).collect::<String>()),
            ),
        };
        webhook_delivery::Entity::update_many()
            .col_expr(
                webhook_delivery::Column::Attempts,
                Expr::col(webhook_delivery::Column::Attempts).add(1),
            )
            .col_expr(webhook_delivery::Column::NextAttemptAt, Expr::value(next))
            .col_expr(
                webhook_delivery::Column::DeliveredAt,
                Expr::value(delivered),
            )
            .col_expr(
                webhook_delivery::Column::LastStatus,
                Expr::value(status.map(i32::from)),
            )
            .col_expr(webhook_delivery::Column::LastError, Expr::value(error))
            .filter(webhook_delivery::Column::Id.eq(id.to_string()))
            .exec(&self.connection)
            .await?;
        Ok(())
    }

    /// A webhook's most recent events, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`WebhookRepositoryError`] if the query fails.
    pub async fn deliveries(
        &self,
        webhook: WebhookId,
        limit: u64,
    ) -> Result<Vec<Delivery>, WebhookRepositoryError> {
        webhook_delivery::Entity::find()
            .filter(webhook_delivery::Column::WebhookId.eq(webhook.to_string()))
            .order_by_desc(webhook_delivery::Column::Id)
            .limit(limit)
            .all(&self.connection)
            .await?
            .into_iter()
            .map(|row| {
                Ok(Delivery {
                    id: WebhookDeliveryId::parse(&row.id)
                        .map_err(|_| WebhookRepositoryError::Corrupt(row.id.clone()))?,
                    event: row.event,
                    attempts: u32::try_from(row.attempts).unwrap_or(0),
                    next_attempt_at: row.next_attempt_at,
                    delivered_at: row.delivered_at,
                    last_status: row
                        .last_status
                        .and_then(|status| u16::try_from(status).ok()),
                    last_error: row.last_error,
                    created_at: row.created_at,
                })
            })
            .collect()
    }

    /// Deletes events that happened before `cutoff` and are no longer being
    /// sent.
    ///
    /// # Errors
    ///
    /// Returns [`sea_orm::DbErr`] if the delete fails.
    pub async fn delete_finished_before(
        &self,
        cutoff: OffsetDateTime,
    ) -> Result<u64, sea_orm::DbErr> {
        Ok(webhook_delivery::Entity::delete_many()
            .filter(webhook_delivery::Column::NextAttemptAt.is_null())
            .filter(webhook_delivery::Column::CreatedAt.lt(cutoff))
            .exec(&self.connection)
            .await?
            .rows_affected)
    }

    fn open(&self, row: &webhook::Model) -> Result<Webhook, WebhookRepositoryError> {
        let corrupt = || WebhookRepositoryError::Corrupt(row.id.clone());
        Ok(Webhook {
            id: WebhookId::parse(&row.id).map_err(|_| corrupt())?,
            owner: row
                .user_id
                .as_deref()
                .map(UserId::parse)
                .transpose()
                .map_err(|_| corrupt())?,
            url: self.sealer.open(&row.url)?,
            events: row
                .events
                .split_whitespace()
                .map(|name| Event::parse(name).ok_or_else(corrupt))
                .collect::<Result<_, _>>()?,
            format: Format::parse(&row.format).ok_or_else(corrupt)?,
            secret: self.sealer.open(&row.secret)?,
            created_at: row.created_at,
        })
    }
}

/// The rows of one account, or the instance's own for `None`.
fn owned_by(owner: Option<UserId>) -> Condition {
    match owner {
        Some(owner) => Condition::all().add(webhook::Column::UserId.eq(owner.to_string())),
        None => Condition::all().add(webhook::Column::UserId.is_null()),
    }
}
