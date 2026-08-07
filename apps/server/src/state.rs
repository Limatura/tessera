//! Shared application state.

use std::sync::Arc;

use object_store::ObjectStore;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;

use crate::{config::Config, db::Database};

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
    /// Relational store.
    pub db: Database,
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
    pub fn new(config: Arc<Config>, db: Database, storage: Arc<dyn ObjectStore>) -> Self {
        Self {
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
