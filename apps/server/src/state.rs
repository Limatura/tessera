//! Shared application state.

use std::sync::Arc;

use arc_swap::ArcSwap;
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
        access_event::AccessEventRepository,
        api_token::ApiTokenRepository,
        invite::InviteRepository,
        oidc::OidcRepository,
        password_reset::PasswordResetRepository,
        pending_registration::PendingRegistrationRepository,
        preference::PreferenceRepository,
        role::RoleRepository,
        session::SessionRepository,
        setting::SettingRepository,
        share::{ShareRepository, ShareTicketRepository},
        upload::UploadRepository,
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

/// Everything about where blobs go, as one argument.
///
/// The store, its multipart view, the pool and the key that opens the pool's
/// rows are one subject: taking three from here and the fourth from elsewhere
/// gives a state whose backends and whose key disagree.
#[derive(Debug, Clone)]
pub struct Storage {
    /// Where new content goes when nothing else claims it.
    pub default: crate::storage::Stores,
    /// Every backend that opened, and what they were opened from.
    pub pool: crate::storage::Pool,
    /// What seals and opens a stored backend's credentials.
    pub sealer: crate::storage::secret::Sealer,
}

/// State cloned into every request handler.
///
/// Cheap to clone: everything inside is an `Arc` or an `Arc`-backed pool.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Validated configuration, as it stands right now.
    ///
    /// Behind a swap, not a plain `Arc`, because a change made in the
    /// admin panel has to reach the next request without a restart. Read it
    /// through [`AppState::config`], and do not hold the result across an
    /// await, or it will be acting on a configuration since replaced.
    config: Arc<ArcSwap<Config>>,
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
    /// Settings changed from the admin panel.
    pub settings: SettingRepository,
    /// Recorded retrievals, when the operator has asked for them. Written only
    /// while `share.access_log.enabled` is on.
    pub events: AccessEventRepository,
    /// Outstanding password reset links.
    pub resets: PasswordResetRepository,
    /// Invite codes, for an instance that is neither open nor closed.
    pub invites: InviteRepository,
    /// Registrations waiting on their address to be confirmed.
    pub pending: PendingRegistrationRepository,
    /// Provider identities, and the sign-ins that are away at the provider.
    pub identities: OidcRepository,
    /// What each person has chosen about how the site looks to them.
    pub preferences: PreferenceRepository,
    /// The relay, when this instance has one.
    ///
    /// `None` is the ordinary case and not a fault: an instance with no `mail`
    /// block does not mount the endpoints that would use it.
    ///
    /// Swappable for the same reason the configuration is. `mail.host` and its
    /// neighbours are settings the panel can change, and a relay left pointed
    /// at the old server would report success and send nothing.
    mailer: Arc<ArcSwap<Option<Mailer>>>,
    /// The proxies this instance will be spoken for by.
    ///
    /// Parsed once at start-up, not per request. It is read on every request
    /// with a throttle behind it, for the same answer every time.
    proxies: Arc<ArcSwap<TrustedProxies>>,
    /// Failed-attempt counters.
    ///
    /// Shared, not per-request, since the point is that attempts accumulate
    /// across them. In memory, so this process owns it and loses it
    /// on restart; see [`Throttles`] for why, and why there is more than one
    /// counter.
    pub throttles: Arc<Throttles>,
    /// Blob store: the one new content is written to.
    ///
    /// Reading goes through [`AppState::backend`] instead, because a file
    /// records which backend holds it and that may not be this one any more.
    pub storage: Arc<dyn ObjectStore>,
    /// The stored backends, and the key that opens them.
    ///
    /// Holds the sealer, so the one place that can read a credential is the one
    /// place that needs to. Nothing above this layer sees the key.
    pub storage_backends: crate::db::repository::storage_backend::StorageBackendRepository,
    /// Every configured backend, by name.
    ///
    /// An `ArcSwap` because the set is meant to be changeable while the process
    /// runs, the same shape as `mailer`. Today it is built once from
    /// `storage.url` and holds a single entry.
    pub backends: Arc<ArcSwap<crate::storage::Pool>>,
    /// Keeps the first upload into a location that does not exist yet from
    /// racing its siblings. See
    /// [`FirstWrites`](crate::storage::first_write::FirstWrites).
    pub first_writes: Arc<crate::storage::first_write::FirstWrites>,
    /// Bytes arriving right now, which no table knows about yet. See
    /// [`InFlight`](crate::storage::in_flight::InFlight).
    pub in_flight: Arc<crate::storage::in_flight::InFlight>,
    /// The same store when it can hold a multipart upload open across requests.
    ///
    /// `None` on a local filesystem, where a resumable upload appends to a
    /// partial file instead. See `upload::sink`.
    pub multipart: Option<crate::storage::Multipart>,
    /// Upload sessions.
    pub uploads: UploadRepository,
    /// Where the frontend is read from: the compiled-in bundle, or a directory.
    pub assets: Arc<crate::assets::Source>,
    /// What the identity provider says about itself, fetched once a day.
    ///
    /// Held here rather than built per request so that one sign-in pays for the
    /// fetch and the rest of the day's do not.
    pub oidc: Arc<crate::auth::oidc::discovery::Discovery>,
    /// The provider's signing keys, for back-channel logout only.
    ///
    /// Separate from [`Self::oidc`] because they are fetched for a different
    /// reason and from a different URL, and because most instances never touch
    /// them at all.
    pub oidc_keys: Arc<crate::auth::oidc::jwks::Keys>,
    /// Fonts for link-preview cards, decoded once at start-up.
    ///
    /// `None` when the embedded faces would not decode. That is a build fault
    /// and should not stop an instance serving files, so it costs the pictures
    /// and nothing else: the route answers `404` and the page keeps its words.
    #[cfg(feature = "og-images")]
    pub cards: Option<Arc<crate::og::Cards>>,
    /// Current lifecycle status.
    ///
    /// A `parking_lot::RwLock`, not an async lock. The critical section is a
    /// single enum write, so an await point here costs more than it saves.
    status: Arc<RwLock<ServiceStatus>>,
    /// When the process started, used for the uptime figure in `/health`.
    started_at: OffsetDateTime,
}

impl AppState {
    /// The configuration as it stands right now.
    ///
    /// Cheap: a pointer read, not a lock. The guard must not be held across an
    /// await point, or the handler is reading a configuration that may already
    /// have been replaced, and the compiler will not say so.
    #[must_use]
    pub fn config(&self) -> arc_swap::Guard<Arc<Config>> {
        self.config.load()
    }

    /// The relay, when this instance has one.
    ///
    /// `None` is the ordinary case and not a fault: an instance with no `mail`
    /// block does not mount the endpoints that would use it.
    #[must_use]
    pub fn mailer(&self) -> arc_swap::Guard<Arc<Option<Mailer>>> {
        self.mailer.load()
    }

    /// The proxies this instance will be spoken for by.
    #[must_use]
    pub fn proxies(&self) -> arc_swap::Guard<Arc<TrustedProxies>> {
        self.proxies.load()
    }

    /// The store holding a named backend's objects.
    ///
    /// `None` when the name is not configured: a share whose backend
    /// was removed. That has to be answerable, not fatal: the operator
    /// needs to be told which shares they orphaned, not have the process refuse
    /// to start.
    #[must_use]
    pub fn backend(&self, name: &str) -> Option<Arc<dyn ObjectStore>> {
        self.backends
            .load()
            .open
            .get(name)
            .map(|stores| Arc::clone(&stores.store))
    }

    /// The same, as a multipart store when that backend is one.
    ///
    /// `None` for a name that is not configured *and* for one that cannot hold
    /// a multipart open, such as a local directory or a plain HTTP store. The
    /// caller tells them apart by asking [`AppState::backend`] first.
    #[must_use]
    pub fn backend_multipart(&self, name: &str) -> Option<crate::storage::Multipart> {
        self.backends
            .load()
            .open
            .get(name)
            .and_then(|stores| stores.multipart.clone())
    }

    /// Replaces the set of backends, so one added from the panel or the command
    /// line is reachable without a restart.
    ///
    /// The map is swapped whole, not mutated. A request already reading
    /// holds its own `Arc` and finishes against the set it started with, which
    /// is what makes this safe under load.
    pub fn replace_backends(&self, pool: crate::storage::Pool) {
        self.backends.store(Arc::new(pool));
    }

    /// Replaces the configuration, and everything built from it.
    ///
    /// # Why the derived things are rebuilt here
    ///
    /// Swapping the configuration alone would leave the mailer talking to the
    /// old relay and the proxy list trusting the old addresses: a change the
    /// panel reported as saved that visibly did nothing. Everything derived
    /// from a setting is rebuilt in the same call, so there is one place to
    /// look when something turns out not to follow.
    ///
    /// The throttles are updated, not rebuilt, because they hold the
    /// count of who has been failing to sign in. See [`Throttles::apply`].
    pub fn replace_config(&self, config: Arc<Config>) {
        // The cached document belongs to the issuer it was fetched from. A new
        // issuer means the endpoints, and the name we check `iss` against, are
        // somebody else's.
        if config.auth.oidc.issuer != self.config().auth.oidc.issuer {
            self.oidc.forget();
            self.oidc_keys.forget();
        }
        self.throttles.apply(&config.auth.throttle);
        self.proxies.store(Arc::new(config.server.proxy_trust()));
        match Mailer::from_config(&config.mail) {
            Ok(mailer) => self.mailer.store(Arc::new(mailer)),
            Err(error) => {
                tracing::error!(
                    ?error,
                    "the new mail settings are unusable; keeping the old relay"
                );
            }
        }
        self.config.store(config);
    }

    /// Assembles state for a process that has connected its dependencies but is
    /// not yet serving traffic.
    #[must_use]
    pub fn new(
        config: Arc<Config>,
        db: DatabaseConnection,
        storage: Storage,
        assets: crate::assets::Source,
    ) -> Self {
        let Storage {
            default:
                crate::storage::Stores {
                    store: storage,
                    multipart,
                    ..
                },
            pool: backends,
            sealer,
        } = storage;
        let storage_backends =
            crate::db::repository::storage_backend::StorageBackendRepository::new(
                db.clone(),
                sealer.clone(),
            );
        // A relay that will not build is reported and then treated as absent.
        // Refusing to start would take the service down over the one feature
        // that exists for when something else has already gone wrong, and the
        // endpoints check for `Some` anyway.
        let mailer = match Mailer::from_config(&config.mail) {
            Ok(mailer) => mailer,
            Err(error) => {
                tracing::error!(?error, "mail is configured but unusable; sending is off");
                None
            }
        };

        Self {
            first_writes: Arc::default(),
            in_flight: Arc::default(),
            users: UserRepository::new(db.clone()),
            roles: RoleRepository::new(db.clone()),
            resets: PasswordResetRepository::new(db.clone()),
            invites: InviteRepository::new(db.clone()),
            pending: PendingRegistrationRepository::new(db.clone()),
            identities: OidcRepository::new(db.clone()),
            preferences: PreferenceRepository::new(db.clone()),
            mailer: Arc::new(ArcSwap::from_pointee(mailer)),
            sessions: SessionRepository::new(db.clone()),
            tokens: ApiTokenRepository::new(db.clone()),
            shares: ShareRepository::new(db.clone()),
            tickets: ShareTicketRepository::new(db.clone()),
            settings: SettingRepository::new(db.clone(), sealer),
            events: AccessEventRepository::new(db.clone()),
            uploads: UploadRepository::new(db.clone()),
            multipart,
            assets: Arc::new(assets),
            oidc: Arc::new(crate::auth::oidc::discovery::Discovery::default()),
            oidc_keys: Arc::new(crate::auth::oidc::jwks::Keys::default()),
            #[cfg(feature = "og-images")]
            cards: match crate::og::Cards::new() {
                Ok(cards) => Some(Arc::new(cards)),
                Err(error) => {
                    tracing::error!(%error, "link-preview cards are unavailable");
                    None
                }
            },
            proxies: Arc::new(ArcSwap::from_pointee(config.server.proxy_trust())),
            throttles: Arc::new(Throttles::from_config(&config.auth.throttle)),
            config: Arc::new(ArcSwap::from(config)),
            db,
            storage_backends,
            backends: Arc::new(ArcSwap::from_pointee(backends)),
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

    /// Whole seconds since start-up. Saturates instead of wrapping if the
    /// system clock steps backwards.
    #[must_use]
    pub fn uptime_seconds(&self) -> u64 {
        let elapsed = OffsetDateTime::now_utc() - self.started_at;
        u64::try_from(elapsed.whole_seconds()).unwrap_or(0)
    }
}
