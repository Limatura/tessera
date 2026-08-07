//! Connection pool and migration runner.
//!
//! The pool is an enum rather than `sqlx::Any` on purpose: `Any` erases the
//! backend, which means losing SQLite-specific pragmas (WAL, `foreign_keys`,
//! `busy_timeout`) and the compile-time-checked `query!` macros that the domain
//! layer will want in the next round. The cost is one `match` per call site,
//! which is contained by keeping every query behind this module.

use std::str::FromStr;

use sqlx::{
    migrate::Migrator,
    sqlite::{
        SqliteAutoVacuum, SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions,
        SqliteSynchronous,
    },
};

use crate::config::DatabaseConfig;

/// Migrations for the SQLite dialect.
static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("migrations/sqlite");

/// Migrations for the Postgres dialect.
#[cfg(feature = "postgres")]
static POSTGRES_MIGRATOR: Migrator = sqlx::migrate!("migrations/postgres");

/// Which dialect a connection URL selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// `sqlite:` URLs.
    Sqlite,
    /// `postgres:` / `postgresql:` URLs.
    Postgres,
}

impl Backend {
    /// Classifies a connection URL, or `None` if the scheme is unsupported.
    ///
    /// Lives here rather than in `config` so that the set of understood schemes
    /// and the set of pools we can actually build cannot drift apart.
    #[must_use]
    pub fn from_url(url: &str) -> Option<Self> {
        let scheme = url.split_once(':').map(|(scheme, _)| scheme)?;
        match scheme {
            "sqlite" => Some(Self::Sqlite),
            "postgres" | "postgresql" => Some(Self::Postgres),
            _ => None,
        }
    }

    /// Short name for logs and health output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sqlite => "sqlite",
            Self::Postgres => "postgres",
        }
    }
}

/// A live connection pool for whichever backend the configuration selected.
#[derive(Debug, Clone)]
pub enum Database {
    /// SQLite pool.
    Sqlite(sqlx::SqlitePool),
    /// Postgres pool.
    #[cfg(feature = "postgres")]
    Postgres(sqlx::PgPool),
}

impl Database {
    /// Opens a pool described by `config`.
    ///
    /// The URL scheme has already been checked by configuration validation; an
    /// unsupported scheme reaching this point is reported as a connection error
    /// rather than a panic.
    ///
    /// # Errors
    ///
    /// Returns [`sqlx::Error`] if the pool cannot be created or the first
    /// connection cannot be established.
    pub async fn connect(config: &DatabaseConfig) -> Result<Self, sqlx::Error> {
        match Backend::from_url(&config.url) {
            Some(Backend::Sqlite) => Self::connect_sqlite(config).await,
            Some(Backend::Postgres) => Self::connect_postgres(config).await,
            None => Err(sqlx::Error::Configuration(
                format!("unsupported database URL scheme in `{}`", config.url).into(),
            )),
        }
    }

    async fn connect_sqlite(config: &DatabaseConfig) -> Result<Self, sqlx::Error> {
        let options = SqliteConnectOptions::from_str(&config.url)?
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

        // `create_if_missing` creates the database *file*, not the directory
        // holding it — so the shipped default of `sqlite://data/tessera.db` would
        // fail on a fresh checkout. A homelab install should not have to mkdir
        // before it can start.
        let filename = options.get_filename();
        if let Some(parent) = filename.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).map_err(sqlx::Error::Io)?;
        }

        let pool = SqlitePoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.acquire_timeout())
            .connect_with(options)
            .await?;
        Ok(Self::Sqlite(pool))
    }

    #[cfg(feature = "postgres")]
    async fn connect_postgres(config: &DatabaseConfig) -> Result<Self, sqlx::Error> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .acquire_timeout(config.acquire_timeout())
            .connect(&config.url)
            .await?;
        Ok(Self::Postgres(pool))
    }

    #[cfg(not(feature = "postgres"))]
    #[expect(
        clippy::unused_async,
        reason = "keeps one signature across both feature configurations"
    )]
    async fn connect_postgres(_config: &DatabaseConfig) -> Result<Self, sqlx::Error> {
        Err(sqlx::Error::Configuration(
            "this binary was built without the `postgres` cargo feature".into(),
        ))
    }

    /// Which backend this pool talks to.
    #[must_use]
    pub const fn backend(&self) -> Backend {
        match self {
            Self::Sqlite(_) => Backend::Sqlite,
            #[cfg(feature = "postgres")]
            Self::Postgres(_) => Backend::Postgres,
        }
    }

    /// Applies every pending migration for this backend.
    ///
    /// # Errors
    ///
    /// Returns [`sqlx::migrate::MigrateError`] if a migration fails or if an
    /// already-applied migration's checksum no longer matches.
    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        match self {
            Self::Sqlite(pool) => SQLITE_MIGRATOR.run(pool).await,
            #[cfg(feature = "postgres")]
            Self::Postgres(pool) => POSTGRES_MIGRATOR.run(pool).await,
        }
    }

    /// Round-trips a trivial query to prove the pool is usable.
    ///
    /// # Errors
    ///
    /// Returns [`sqlx::Error`] if no connection can be acquired or the query fails.
    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        match self {
            Self::Sqlite(pool) => sqlx::query("SELECT 1").execute(pool).await.map(|_| ()),
            #[cfg(feature = "postgres")]
            Self::Postgres(pool) => sqlx::query("SELECT 1").execute(pool).await.map(|_| ()),
        }
    }

    /// Drains and closes the pool. Called on graceful shutdown so in-flight
    /// SQLite writes are checkpointed before the process exits.
    pub async fn close(&self) {
        match self {
            Self::Sqlite(pool) => pool.close().await,
            #[cfg(feature = "postgres")]
            Self::Postgres(pool) => pool.close().await,
        }
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
    #[case("mysql://localhost/tessera", None)]
    #[case("data/tessera.db", None)]
    #[case("", None)]
    fn classifies_urls(#[case] url: &str, #[case] expected: Option<Backend>) {
        assert_eq!(Backend::from_url(url), expected);
    }

    #[tokio::test]
    async fn sqlite_pool_migrates_and_pings() {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            ..Default::default()
        };
        let db = Database::connect(&config).await.unwrap();
        assert_eq!(db.backend(), Backend::Sqlite);
        db.migrate().await.unwrap();
        db.ping().await.unwrap();
        db.close().await;
    }

    #[tokio::test]
    async fn sqlite_creates_missing_parent_directories() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("does/not/exist/tessera.db");
        let config = DatabaseConfig {
            url: format!("sqlite://{}", path.display()),
            ..Default::default()
        };

        let db = Database::connect(&config).await.unwrap();
        db.migrate().await.unwrap();
        db.close().await;

        assert!(
            path.is_file(),
            "the database file should have been created at {}",
            path.display()
        );
    }

    #[tokio::test]
    async fn migrations_are_idempotent() {
        let config = DatabaseConfig {
            url: "sqlite::memory:".to_owned(),
            ..Default::default()
        };
        let db = Database::connect(&config).await.unwrap();
        db.migrate().await.unwrap();
        db.migrate().await.unwrap();
        db.close().await;
    }
}
