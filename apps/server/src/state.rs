//! Shared application state.

use std::sync::Arc;

use object_store::ObjectStore;
use parking_lot::RwLock;
use sea_orm::DatabaseConnection;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{
    auth::{proxy::TrustedProxies, throttle::Throttles},
    config::Config,
    db::repository::{
        api_token::ApiTokenRepository,
        invite::InviteRepository,
        password_reset::PasswordResetRepository,
        pending_registration::PendingRegistrationRepository,
        role::RoleRepository,
        session::SessionRepository,
        share::{ShareRepository, ShareTicketRepository},
        user::UserRepository,
    },
    mail::Mailer,
};

/// Where the process is in its lifecycle.
///
/// Load balancers and `docker healthcheck` need to distinguish "still migrating"
/// from "draining" from "healthy"; a bare 200/500 cannot express that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceStatus {
    /// Booting: connecting to the database, running migrations.
    Starting,
    /// Accepting traffic.
    Ready,
    /// Refusing new work while in-flight requests finish.
    Draining,
}

/// State cloned into every request handler.
///
/// Cheap to clone: everything inside is an `Arc` or an `Arc`-backed pool.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Validated configuration.
    pub config: Arc<Config>,
    /// Relational store. One connection type covers SQLite, Postgres and MySQL.
    pub db: DatabaseConnection,
    /// Accounts.
    pub users: UserRepository,
    /// Roles, and what each one allows.
    pub roles: RoleRepository,
    /// Browser sessions.
    pub sessions: SessionRepository,
    /// API tokens.
    pub tokens: ApiTokenRepository,
    /// Shares.
    pub shares: ShareRepository,
    /// Download authorisations.
    pub tickets: ShareTicketRepository,
    /// Outstanding password reset links.
    pub resets: PasswordResetRepository,
    /// Invite codes, for an instance that is neither open nor closed.
    pub invites: InviteRepository,
    /// Registrations waiting on their address to be confirmed.
    pub pending: PendingRegistrationRepository,
    /// The relay, when this instance has one.
    ///
    /// `None` is the ordinary case and not a fault: an instance with no `mail`
    /// block does not mount the endpoints that would use it.
    pub mailer: Option<Mailer>,
    /// The proxies this instance will be spoken for by.
    ///
    /// Parsed once at start-up rather than per request: it is read on every
    /// request that has a throttle behind it, and the parsing is the same answer
    /// every time.
    pub proxies: Arc<TrustedProxies>,
    /// Failed-attempt counters.
    ///
    /// Shared rather than per-request: the whole point is that attempts
    /// accumulate across requests. In memory, so it is state this process owns
    /// and loses on restart — see [`Throttles`] for why that is the right trade,
    /// and why there is more than one counter.
    pub throttles: Arc<Throttles>,
    /// Blob store.
    pub storage: Arc<dyn ObjectStore>,
    /// Current lifecycle status.
    ///
    /// A `parking_lot::RwLock` rather than an async lock: the critical section is
    /// a single enum write, so an await point here would cost more than it saves.
    status: Arc<RwLock<ServiceStatus>>,
    /// When the process started, used for the uptime figure in `/health`.
    started_at: OffsetDateTime,
}

impl AppState {
    /// Assembles state for a process that has connected its dependencies but is
    /// not yet serving traffic.
    #[must_use]
    pub fn new(config: Arc<Config>, db: DatabaseConnection, storage: Arc<dyn ObjectStore>) -> Self {
        // A relay that will not build is reported and then treated as absent.
        // Refusing to start would take the whole service down over the one
        // feature that exists to be used when something else has already gone
        // wrong — and the endpoints check for `Some` anyway.
        let mailer = match Mailer::from_config(&config.mail) {
            Ok(mailer) => mailer,
            Err(error) => {
                tracing::error!(?error, "mail is configured but unusable; sending is off");
                None
            }
        };

        Self {
            users: UserRepository::new(db.clone()),
            roles: RoleRepository::new(db.clone()),
            resets: PasswordResetRepository::new(db.clone()),
            invites: InviteRepository::new(db.clone()),
            pending: PendingRegistrationRepository::new(db.clone()),
            mailer,
            sessions: SessionRepository::new(db.clone()),
            tokens: ApiTokenRepository::new(db.clone()),
            shares: ShareRepository::new(db.clone()),
            tickets: ShareTicketRepository::new(db.clone()),
            proxies: Arc::new(config.server.proxy_trust()),
            throttles: Arc::new(Throttles::default()),
            config,
            db,
            storage,
            status: Arc::new(RwLock::new(ServiceStatus::Starting)),
            started_at: OffsetDateTime::now_utc(),
        }
    }

    /// Current lifecycle status.
    #[must_use]
    pub fn status(&self) -> ServiceStatus {
        *self.status.read()
    }

    /// Transitions to a new lifecycle status.
    pub fn set_status(&self, status: ServiceStatus) {
        *self.status.write() = status;
    }

    /// When the process started.
    #[must_use]
    pub const fn started_at(&self) -> OffsetDateTime {
        self.started_at
    }

    /// Whole seconds since start-up. Saturates rather than wrapping if the system
    /// clock steps backwards.
    #[must_use]
    pub fn uptime_seconds(&self) -> u64 {
        let elapsed = OffsetDateTime::now_utc() - self.started_at;
        u64::try_from(elapsed.whole_seconds()).unwrap_or(0)
    }
}
