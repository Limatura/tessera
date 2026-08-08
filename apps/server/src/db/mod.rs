//! Connection pool and migration runner.
//!
//! One [`DatabaseConnection`] covers SQLite, Postgres and MySQL. That is the
//! reason this project uses SeaORM at all: the previous shape was an enum with a
//! `match` in every method, which costs one arm per backend per query — twenty
//! queries across three dialects is sixty code paths, of which two thirds are
//! rarely exercised.
//!
//! What that trade gives up is `sqlx::query!`'s compile-time schema check. It is
//! worth being precise about the loss: that macro only ever verified *one*
//! dialect, so committing to three removed it regardless of ORM. What replaces
//! it is CI — the repository tests run against all three real servers, which is
//! narrower feedback but wider coverage.

pub mod entity;
pub mod migration;
pub mod repository;

use sea_orm::{ConnectOptions, DatabaseConnection, DbBackend};
use sea_orm_migration::MigratorTrait as _;

use crate::config::DatabaseConfig;

/// Which dialect a connection URL selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// `sqlite:` URLs.
    Sqlite,
    /// `postgres:` / `postgresql:` URLs.
    Postgres,
    /// `mysql:` / `mariadb:` URLs.
    MySql,
}

impl Backend {
    /// Classifies a connection URL, or `None` if the scheme is unsupported.
    ///
    /// Lives here rather than in `config` so that the set of understood schemes
    /// and the set of pools we can actually build cannot drift apart.
    #[must_use]
    pub fn from_url(url: &str) -> Option<Self> {
        match url.split_once(':').map(|(scheme, _)| scheme)? {
            "sqlite" => Some(Self::Sqlite),
            "postgres" | "postgresql" => Some(Self::Postgres),
            "mysql" | "mariadb" => Some(Self::MySql),
            _ => None,
        }
    }

    /// Cargo feature that must be enabled for this backend to be usable.
    #[must_use]
    pub const fn required_feature(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
            Self::MySql => "mysql",
        }
    }

    /// Whether this binary was built with support for the backend.
    #[must_use]
    pub const fn is_compiled_in(self) -> bool {
        match self {
            Self::Sqlite => cfg!(feature = "sqlite"),
            Self::Postgres => cfg!(feature = "postgres"),
            Self::MySql => cfg!(feature = "mysql"),
        }
    }

    /// Short name for logs and health output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
            Self::MySql => "mysql",
        }
    }
}

impl Backend {
    /// Classifies a live connection's backend.
    ///
    /// Fallible rather than a `From` impl because `DbBackend` is
    /// `#[non_exhaustive]` — SeaORM may add a backend before we do, and
    /// pretending an unknown one is SQLite would be a lie the health endpoint
    /// then repeats to an operator.
    #[must_use]
    pub const fn from_sea_orm(backend: DbBackend) -> Option<Self> {
        match backend {
            DbBackend::Sqlite => Some(Self::Sqlite),
            DbBackend::Postgres => Some(Self::Postgres),
            DbBackend::MySql => Some(Self::MySql),
            _ => None,
        }
    }
}

/// Opens a connection described by `config`.
///
/// # Errors
///
/// Returns [`sea_orm::DbErr`] if the URL scheme is unsupported, the backend was
/// not compiled in, or the first connection cannot be established.
pub async fn connect(config: &DatabaseConfig) -> Result<DatabaseConnection, sea_orm::DbErr> {
    let backend = Backend::from_url(&config.url).ok_or_else(|| {
        sea_orm::DbErr::Custom(format!(
            "unsupported database URL scheme in `{}`",
            config.url
        ))
    })?;

    if !backend.is_compiled_in() {
        return Err(sea_orm::DbErr::Custom(format!(
            "this binary was built without the `{}` cargo feature",
            backend.required_feature()
        )));
    }

    match backend {
        Backend::Sqlite => connect_sqlite(config).await,
        Backend::Postgres | Backend::MySql => connect_networked(config).await,
    }
}

/// Postgres and MySQL: SeaORM's own pool is enough, because everything we want
/// to tune (`max_connections`, timeouts) is exposed by [`ConnectOptions`].
async fn connect_networked(config: &DatabaseConfig) -> Result<DatabaseConnection, sea_orm::DbErr> {
    let mut options = ConnectOptions::new(config.url.clone());
    options
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout())
        .connect_timeout(config.acquire_timeout())
        // SeaORM logs every statement at `info` by default, which is far too
        // loud for a service and would put retrieval codes in the log.
        .sqlx_logging(false);
    sea_orm::Database::connect(options).await
}

/// SQLite: build the sqlx pool by hand, then hand it to SeaORM.
///
/// [`ConnectOptions`] cannot express SQLite pragmas and sqlx's URL parser
/// rejects unknown query parameters, so there is no way to ask for WAL through
/// SeaORM's own API. Setting the pragmas after connecting does not work either —
/// `journal_mode` is persistent but `foreign_keys`, `busy_timeout` and
/// `synchronous` are per-connection, and the pool opens connections lazily.
#[cfg(feature = "sqlite")]
async fn connect_sqlite(config: &DatabaseConfig) -> Result<DatabaseConnection, sea_orm::DbErr> {
    use std::str::FromStr as _;

    use sqlx::sqlite::{
        SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    };

    let options = SqliteConnectOptions::from_str(&config.url)
        .map_err(|e| sea_orm::DbErr::Conn(sea_orm::RuntimeErr::Internal(e.to_string())))?
        // A fresh homelab install should not have to run a setup command.
        .create_if_missing(true)
        // WAL lets readers proceed during a write, which matters because
        // downloads and the expiry sweeper will overlap.
        .journal_mode(SqliteJournalMode::Wal)
        // `Normal` is the durability/throughput point WAL is designed for.
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true)
        // Reclaim space after the expiry sweeper deletes rows.
        .auto_vacuum(SqliteAutoVacuum::Incremental)
        .busy_timeout(config.acquire_timeout());

    // `create_if_missing` creates the database *file*, not the directory holding
    // it — so the shipped default of `sqlite://data/tessera.db` would fail on a
    // fresh checkout. A homelab install should not have to mkdir before it starts.
    let filename = options.get_filename();
    if let Some(parent) = filename.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(|e| sea_orm::DbErr::Conn(sea_orm::RuntimeErr::Internal(e.to_string())))?;
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(config.acquire_timeout())
        .connect_with(options)
        .await
        .map_err(|e| sea_orm::DbErr::Conn(sea_orm::RuntimeErr::Internal(e.to_string())))?;

    Ok(sea_orm::SqlxSqliteConnector::from_sqlx_sqlite_pool(pool))
}

#[cfg(not(feature = "sqlite"))]
#[expect(
    clippy::unused_async,
    reason = "keeps one signature across both feature configurations"
)]
async fn connect_sqlite(_config: &DatabaseConfig) -> Result<DatabaseConnection, sea_orm::DbErr> {
    Err(sea_orm::DbErr::Custom(
        "this binary was built without the `sqlite` cargo feature".to_owned(),
    ))
}

/// Applies every pending migration.
///
/// # Errors
///
/// Returns [`sea_orm::DbErr`] if a migration fails or if an already-applied
/// migration's checksum no longer matches.
pub async fn migrate(conn: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    migration::Migrator::up(conn, None).await
}

/// Round-trips a trivial query to prove the connection is usable.
///
/// # Errors
///
/// Returns [`sea_orm::DbErr`] if no connection can be acquired or the query fails.
pub async fn ping(conn: &DatabaseConnection) -> Result<(), sea_orm::DbErr> {
    conn.ping().await
}

/// Which backend a live connection talks to, or `None` for one this build does
/// not recognise.
#[must_use]
pub fn backend_of(conn: &DatabaseConnection) -> Option<Backend> {
    Backend::from_sea_orm(conn.get_database_backend())
}

/// Backend name for logs and health output, or `unknown`.
#[must_use]
pub fn backend_name(conn: &DatabaseConnection) -> &'static str {
    backend_of(conn).map_or("unknown", Backend::as_str)
}

/// Closes the connection. Called on graceful shutdown so in-flight SQLite writes
/// are checkpointed before the process exits.
pub async fn close(conn: DatabaseConnection) {
    if let Err(error) = conn.close().await {
        tracing::warn!(?error, "the database connection did not close cleanly");
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("sqlite://data/tessera.db", Some(Backend::Sqlite))]
    #[case("sqlite::memory:", Some(Backend::Sqlite))]
    #[case("postgres://user:pw@localhost/tessera", Some(Backend::Postgres))]
    #[case("postgresql://localhost/tessera", Some(Backend::Postgres))]
    #[case("mysql://localhost/tessera", Some(Backend::MySql))]
    #[case("mariadb://localhost/tessera", Some(Backend::MySql))]
    #[case("mongodb://localhost/tessera", None)]
    #[case("data/tessera.db", None)]
    #[case("", None)]
    fn classifies_urls(#[case] url: &str, #[case] expected: Option<Backend>) {
        assert_eq!(Backend::from_url(url), expected);
    }

    #[rstest]
    #[case(Backend::Sqlite, "sqlite")]
    #[case(Backend::Postgres, "postgres")]
    #[case(Backend::MySql, "mysql")]
    fn feature_name_matches_backend_name(#[case] backend: Backend, #[case] expected: &str) {
        // The two happen to coincide; the test exists so that renaming a feature
        // without updating `required_feature` is caught.
        assert_eq!(backend.as_str(), expected);
        assert_eq!(backend.required_feature(), expected);
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn sqlite_connects_migrates_and_pings() {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            ..Default::default()
        };
        let conn = connect(&config).await.unwrap();
        assert_eq!(backend_of(&conn), Some(Backend::Sqlite));
        migrate(&conn).await.unwrap();
        ping(&conn).await.unwrap();
        close(conn).await;
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn sqlite_migrations_are_idempotent() {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            ..Default::default()
        };
        let conn = connect(&config).await.unwrap();
        migrate(&conn).await.unwrap();
        migrate(&conn).await.unwrap();
        close(conn).await;
    }

    #[cfg(feature = "sqlite")]
    #[tokio::test]
    async fn sqlite_creates_missing_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("does/not/exist/tessera.db");
        let config = DatabaseConfig {
            url: format!("sqlite://{}", path.display()),
            ..Default::default()
        };

        let conn = connect(&config).await.unwrap();
        migrate(&conn).await.unwrap();
        close(conn).await;

        assert!(
            path.is_file(),
            "expected a database file at {}",
            path.display()
        );
    }

    #[tokio::test]
    async fn an_unsupported_scheme_is_reported_not_panicked() {
        let config = DatabaseConfig {
            url: "mongodb://localhost/tessera".to_owned(),
            ..Default::default()
        };
        let err = connect(&config).await.unwrap_err();
        assert!(err.to_string().contains("unsupported"), "got: {err}");
    }
}
