//! Configuration loading and validation.
//!
//! Precedence, lowest to highest: built-in defaults → optional TOML file →
//! environment variables (`TESSERA_` prefix, `__` separates nesting levels).
//!
//! Every value is validated in [`Config::load`]. A bad log filter or an
//! unsupported database URL therefore fails during start-up, not on the first
//! request that happens to touch it.

use std::{net::IpAddr, path::PathBuf, time::Duration};

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
    /// Connection URL. `sqlite://…` always works; `postgres://…` needs the
    /// `postgres` cargo feature.
    pub url: String,
    /// Upper bound on pooled connections.
    pub max_connections: u32,
    /// How long to wait for a free connection before failing the request.
    pub acquire_timeout_secs: u64,
    /// Run pending migrations during start-up.
    pub run_migrations: bool,
}

/// Which blob backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageBackend {
    /// A directory on the local filesystem.
    Local,
    /// Any S3-compatible endpoint. Requires the `s3` cargo feature.
    S3,
}

/// Blob store settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorageConfig {
    /// Selected backend.
    pub backend: StorageBackend,
    /// Root directory for [`StorageBackend::Local`]. Created if missing.
    pub local_path: PathBuf,
    /// Settings for [`StorageBackend::S3`].
    pub s3: S3Config,
}

/// S3-compatible endpoint settings. Credentials are read from the standard AWS
/// environment variables rather than from this file, so they never land in a
/// config file that might get committed.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct S3Config {
    /// Target bucket.
    pub bucket: String,
    /// Region name. Many self-hosted gateways ignore this but still require it.
    pub region: String,
    /// Custom endpoint, e.g. a MinIO or Garage address.
    pub endpoint: Option<String>,
    /// Address the bucket as a path segment instead of a subdomain. Required by
    /// most self-hosted gateways.
    pub allow_http: bool,
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
            backend: StorageBackend::Local,
            local_path: PathBuf::from("data/blobs"),
            s3: S3Config::default(),
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
        match self.backend {
            StorageBackend::Local => {
                if self.local_path.as_os_str().is_empty() {
                    return Err(ConfigError::Invalid {
                        field: "storage.local_path",
                        reason: "must not be empty when the backend is `local`".to_owned(),
                    });
                }
                Ok(())
            }
            StorageBackend::S3 => {
                if !cfg!(feature = "s3") {
                    return Err(ConfigError::MissingFeature {
                        feature: "s3",
                        detail: "storage.backend is `s3`".to_owned(),
                    });
                }
                if self.s3.bucket.is_empty() {
                    return Err(ConfigError::Invalid {
                        field: "storage.s3.bucket",
                        reason: "must be set when the backend is `s3`".to_owned(),
                    });
                }
                Ok(())
            }
        }
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

    #[test]
    fn missing_config_file_is_not_an_error() {
        let config = Config::load_from("/nonexistent/tessera.toml").unwrap();
        assert_eq!(config, Config::default());
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
    #[case::empty_path("storage.local_path", |c: &mut Config| c.storage.local_path = "".into())]
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

    #[test]
    fn s3_backend_requires_the_s3_feature_or_a_bucket() {
        let mut config = Config::default();
        config.storage.backend = StorageBackend::S3;
        let err = config.validate().unwrap_err();
        if cfg!(feature = "s3") {
            assert!(matches!(
                err,
                ConfigError::Invalid {
                    field: "storage.s3.bucket",
                    ..
                }
            ));
        } else {
            assert!(matches!(
                err,
                ConfigError::MissingFeature { feature: "s3", .. }
            ));
        }
    }
}
