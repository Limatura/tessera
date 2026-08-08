//! Configuration loading and validation.
//!
//! Precedence, lowest to highest: built-in defaults → optional TOML file →
//! environment variables (`TESSERA_` prefix, `__` separates nesting levels).
//!
//! Every value is validated in [`Config::load`]. A bad log filter or an
//! unsupported database URL therefore fails during start-up, not on the first
//! request that happens to touch it.

use std::{collections::BTreeMap, net::IpAddr, time::Duration};

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml},
};
use serde::{Deserialize, Serialize};

/// Prefix for every environment variable that feeds configuration.
pub const ENV_PREFIX: &str = "TESSERA_";

/// Separator used to express nesting in environment variables,
/// e.g. `TESSERA_SERVER__PORT=8080`.
pub const ENV_NESTED_SEPARATOR: &str = "__";

/// Environment variable naming the TOML file to read, if any.
pub const ENV_CONFIG_FILE: &str = "TESSERA_CONFIG_FILE";

/// Path used when [`ENV_CONFIG_FILE`] is unset. Absence is not an error.
pub const DEFAULT_CONFIG_FILE: &str = "tessera.toml";

/// Anything that can go wrong between "process started" and "configuration is
/// known good".
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The file or environment could not be parsed into the expected shape.
    ///
    /// Boxed because `figment::Error` accumulates every failed extraction and is
    /// several hundred bytes wide; unboxed it would set the size of every
    /// `Result<_, ConfigError>` in the crate.
    #[error("could not read configuration: {0}")]
    Source(#[from] Box<figment::Error>),

    /// The shape was right but a value is not usable.
    #[error("invalid configuration: `{field}` — {reason}")]
    Invalid {
        /// Dotted path of the offending field, e.g. `server.port`.
        field: &'static str,
        /// Why it was rejected, phrased for an operator reading a log line.
        reason: String,
    },

    /// The configuration asks for a backend this binary was not built with.
    #[error(
        "configuration requires the `{feature}` cargo feature, which this binary was not built \
         with ({detail})"
    )]
    MissingFeature {
        /// Cargo feature that would enable the requested backend.
        feature: &'static str,
        /// The specific setting that triggered the requirement.
        detail: String,
    },
}

/// Root configuration object.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// HTTP listener and middleware settings.
    pub server: ServerConfig,
    /// Relational store settings.
    pub database: DatabaseConfig,
    /// Blob store settings.
    pub storage: StorageConfig,
    /// Logging and tracing settings.
    pub telemetry: TelemetryConfig,
}

/// HTTP listener and middleware settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Address to bind. `0.0.0.0` inside containers, `127.0.0.1` behind a proxy.
    pub host: IpAddr,
    /// TCP port to bind.
    pub port: u16,
    /// Per-request wall-clock budget, in seconds.
    pub request_timeout_secs: u64,
    /// Maximum accepted request body, in bytes.
    pub max_body_bytes: usize,
    /// Origins allowed to call the API from a browser. Empty means same-origin
    /// only, which is the correct default for the single-binary deployment.
    pub cors_allowed_origins: Vec<String>,
    /// Whether to serve the Swagger UI at `/docs`.
    pub enable_docs: bool,
}

/// Relational store settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    /// Connection URL: `sqlite://…`, `postgres://…` or `mysql://…`. Each scheme
    /// needs its matching cargo feature, all of which are on by default.
    pub url: String,
    /// Upper bound on pooled connections.
    pub max_connections: u32,
    /// How long to wait for a free connection before failing the request.
    pub acquire_timeout_secs: u64,
    /// Run pending migrations during start-up.
    pub run_migrations: bool,
}

/// Blob store settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// Where blobs live.
    ///
    /// A bare path is a local directory, created if missing — the homelab
    /// default. Anything with a scheme is handed to `object_store`:
    /// `s3://bucket/prefix`, `gs://bucket`, `az://container`,
    /// `https://host/dav`, `memory://`.
    pub url: String,

    /// Backend-specific options, passed through verbatim.
    ///
    /// Keys are whatever the chosen backend understands (`aws_region`,
    /// `endpoint`, `allow_http`, …). The standard `AWS_*`, `GOOGLE_*` and
    /// `AZURE_*` environment variables are merged in automatically, so
    /// credentials need not be written into a file that might get committed;
    /// anything set here wins over the environment.
    ///
    /// A `BTreeMap` rather than a `HashMap` so that iteration — and therefore
    /// any log or error listing the options — is deterministic.
    #[serde(default)]
    pub options: BTreeMap<String, String>,
}

/// How log records are rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogFormat {
    /// Human-readable, multi-line. For local development.
    Pretty,
    /// One JSON object per line. For log shippers.
    Json,
}

/// Logging and tracing settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TelemetryConfig {
    /// Output format.
    pub format: LogFormat,
    /// `tracing-subscriber` `EnvFilter` directive, e.g. `info,tessera_server=debug`.
    pub filter: String,
    /// Colourise pretty output. Ignored by [`LogFormat::Json`].
    pub ansi: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::from([127, 0, 0, 1]),
            port: 8787,
            request_timeout_secs: 30,
            // 100 MiB. Generous for a courier service default, and overridable.
            max_body_bytes: 100 * 1024 * 1024,
            cors_allowed_origins: Vec::new(),
            enable_docs: true,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://data/tessera.db".to_owned(),
            max_connections: 8,
            acquire_timeout_secs: 10,
            run_migrations: true,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            url: "data/blobs".to_owned(),
            options: BTreeMap::new(),
        }
    }
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            format: LogFormat::Pretty,
            filter: "info".to_owned(),
            ansi: true,
        }
    }
}

impl ServerConfig {
    /// Per-request timeout as a [`Duration`].
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }
}

impl DatabaseConfig {
    /// Connection acquisition timeout as a [`Duration`].
    #[must_use]
    pub const fn acquire_timeout(&self) -> Duration {
        Duration::from_secs(self.acquire_timeout_secs)
    }
}

impl Config {
    /// Loads and validates configuration from defaults, an optional TOML file and
    /// the environment.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if a source cannot be parsed, if a value is
    /// unusable, or if the configuration selects a backend this binary was not
    /// compiled with.
    pub fn load() -> Result<Self, ConfigError> {
        let file =
            std::env::var(ENV_CONFIG_FILE).unwrap_or_else(|_| DEFAULT_CONFIG_FILE.to_owned());
        Self::load_from(&file)
    }

    /// Same as [`Config::load`], but reads the TOML file from `path`.
    ///
    /// A missing file is not an error — it simply contributes nothing.
    ///
    /// # Errors
    ///
    /// See [`Config::load`].
    pub fn load_from(path: &str) -> Result<Self, ConfigError> {
        let config: Self = Figment::from(Serialized::defaults(Self::default()))
            .merge(Toml::file(path))
            .merge(Env::prefixed(ENV_PREFIX).split(ENV_NESTED_SEPARATOR))
            .extract()
            .map_err(Box::new)?;
        config.validate()?;
        Ok(config)
    }

    /// Rejects any configuration this binary cannot honour.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] for out-of-range or unparsable values and
    /// [`ConfigError::MissingFeature`] when a backend was not compiled in.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.server.validate()?;
        self.database.validate()?;
        self.storage.validate()?;
        self.telemetry.validate()
    }
}

impl ServerConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::Invalid {
                field: "server.port",
                reason: "port 0 asks the OS to pick a port, which makes the service \
                         unaddressable; choose an explicit port"
                    .to_owned(),
            });
        }
        if self.request_timeout_secs == 0 {
            return Err(ConfigError::Invalid {
                field: "server.request_timeout_secs",
                reason: "must be at least 1 second".to_owned(),
            });
        }
        if self.max_body_bytes == 0 {
            return Err(ConfigError::Invalid {
                field: "server.max_body_bytes",
                reason: "must be at least 1 byte".to_owned(),
            });
        }
        for origin in &self.cors_allowed_origins {
            if origin != "*" && !origin.starts_with("http://") && !origin.starts_with("https://") {
                return Err(ConfigError::Invalid {
                    field: "server.cors_allowed_origins",
                    reason: format!(
                        "`{origin}` is not an origin; expected `*` or a scheme-qualified origin \
                         such as `https://example.com`"
                    ),
                });
            }
        }
        Ok(())
    }
}

impl DatabaseConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_connections == 0 {
            return Err(ConfigError::Invalid {
                field: "database.max_connections",
                reason: "must be at least 1".to_owned(),
            });
        }
        if self.acquire_timeout_secs == 0 {
            return Err(ConfigError::Invalid {
                field: "database.acquire_timeout_secs",
                reason: "must be at least 1 second".to_owned(),
            });
        }
        let Some(backend) = crate::db::Backend::from_url(&self.url) else {
            return Err(ConfigError::Invalid {
                field: "database.url",
                reason: "expected a `sqlite:`, `postgres:` or `mysql:` URL".to_owned(),
            });
        };
        if backend.is_compiled_in() {
            Ok(())
        } else {
            Err(ConfigError::MissingFeature {
                feature: backend.required_feature(),
                detail: format!("database.url uses the {} scheme", backend.as_str()),
            })
        }
    }
}

impl StorageConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        // Deliberately shallow. Whether a given scheme is compiled in, and
        // whether its options make sense, is knowledge that lives in
        // `object_store` — duplicating it here would mean a second list to keep
        // in step. `storage::build` surfaces those failures during start-up,
        // which is early enough.
        if self.url.trim().is_empty() {
            return Err(ConfigError::Invalid {
                field: "storage.url",
                reason: "must name a directory or an object-store URL".to_owned(),
            });
        }
        Ok(())
    }
}

impl TelemetryConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        // Parsing here rather than in telemetry::init is the whole point of
        // fail-fast validation: a typo in the filter would otherwise silently
        // degrade to "no logs" long after start-up.
        tracing_subscriber::EnvFilter::try_new(&self.filter).map_err(|e| ConfigError::Invalid {
            field: "telemetry.filter",
            reason: format!("not a valid tracing filter directive: {e}"),
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn defaults_are_valid() {
        Config::default().validate().unwrap();
    }

    /// Wrapped in a `Jail` even though it sets nothing: the jail gives the test a
    /// clean environment *and* takes figment's global lock. Without it this test
    /// reads whatever `TESSERA_*` variables a concurrently running jail happens
    /// to have set — invisible under nextest, which forks per test, and flaky
    /// under any thread-based runner.
    #[test]
    fn missing_config_file_is_not_an_error() {
        figment::Jail::expect_with(|_| {
            let config = Config::load_from("/nonexistent/tessera.toml").expect("no file, no error");
            assert_eq!(config, Config::default());
            Ok(())
        });
    }

    #[rstest]
    #[case::zero_port("server.port", |c: &mut Config| c.server.port = 0)]
    #[case::zero_timeout("server.request_timeout_secs", |c: &mut Config| {
        c.server.request_timeout_secs = 0;
    })]
    #[case::zero_body("server.max_body_bytes", |c: &mut Config| c.server.max_body_bytes = 0)]
    #[case::bad_origin("server.cors_allowed_origins", |c: &mut Config| {
        c.server.cors_allowed_origins = vec!["example.com".to_owned()];
    })]
    #[case::zero_pool("database.max_connections", |c: &mut Config| c.database.max_connections = 0)]
    #[case::bad_url("database.url", |c: &mut Config| c.database.url = "mongodb://x".to_owned())]
    #[case::empty_storage("storage.url", |c: &mut Config| c.storage.url = "  ".to_owned())]
    #[case::bad_filter("telemetry.filter", |c: &mut Config| {
        c.telemetry.filter = "=====".to_owned();
    })]
    fn rejects_bad_values(#[case] field: &str, #[case] mutate: fn(&mut Config)) {
        let mut config = Config::default();
        mutate(&mut config);
        let err = config.validate().unwrap_err();
        assert!(
            err.to_string().contains(field),
            "error for {field} should name the field, got: {err}"
        );
    }

    #[test]
    fn environment_variables_override_defaults_and_files() {
        figment::Jail::expect_with(|jail| {
            jail.create_file(
                "tessera.toml",
                r#"
                [server]
                port = 9001
                [telemetry]
                filter = "warn"
                "#,
            )?;
            // Nested keys use the `__` separator; scalars and lists both parse.
            jail.set_env("TESSERA_SERVER__PORT", "9999");
            jail.set_env("TESSERA_SERVER__ENABLE_DOCS", "false");
            jail.set_env("TESSERA_DATABASE__URL", "sqlite://custom.db");

            let config = Config::load_from("tessera.toml").expect("valid configuration");

            // Environment beats file...
            assert_eq!(config.server.port, 9999);
            assert!(!config.server.enable_docs);
            assert_eq!(config.database.url, "sqlite://custom.db");
            // ...file beats defaults...
            assert_eq!(config.telemetry.filter, "warn");
            // ...and untouched keys keep their defaults.
            assert_eq!(config.server.request_timeout_secs, 30);
            Ok(())
        });
    }

    #[test]
    fn an_invalid_environment_override_fails_at_load_time() {
        figment::Jail::expect_with(|jail| {
            // `EnvFilter` is permissive — it treats most junk as a bare target
            // name. This is one of the shapes it genuinely refuses.
            jail.set_env("TESSERA_TELEMETRY__FILTER", "=====");
            let err = Config::load_from("tessera.toml").expect_err("must be rejected");
            assert!(matches!(
                err,
                ConfigError::Invalid {
                    field: "telemetry.filter",
                    ..
                }
            ));
            Ok(())
        });
    }

    #[test]
    fn unknown_keys_are_rejected_rather_than_ignored() {
        figment::Jail::expect_with(|jail| {
            // A typo in a config file must not silently do nothing.
            jail.create_file("tessera.toml", "[server]\nprot = 9001\n")?;
            let err = Config::load_from("tessera.toml").expect_err("must be rejected");
            assert!(matches!(err, ConfigError::Source(_)), "got {err}");
            Ok(())
        });
    }

    #[rstest]
    #[case("postgres://localhost/tessera", "postgres")]
    #[case("mysql://localhost/tessera", "mysql")]
    #[case("sqlite://data/tessera.db", "sqlite")]
    fn a_backend_this_build_lacks_names_the_missing_feature(
        #[case] url: &str,
        #[case] feature: &str,
    ) {
        let mut config = Config::default();
        config.database.url = url.to_owned();
        let compiled_in =
            crate::db::Backend::from_url(url).is_some_and(crate::db::Backend::is_compiled_in);

        match config.validate() {
            Ok(()) => assert!(compiled_in, "{feature} accepted but not compiled in"),
            Err(err) => {
                assert!(!compiled_in, "{feature} rejected but is compiled in: {err}");
                assert!(
                    err.to_string().contains(feature),
                    "error should name `{feature}`: {err}"
                );
            }
        }
    }
}
