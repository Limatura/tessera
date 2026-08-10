//! Configuration loading and validation.
//!
//! Precedence, lowest to highest: built-in defaults → optional config file →
//! environment variables (`TESSERA_` prefix, `__` separates nesting levels).
//!
//! The file may be YAML or TOML. YAML is looked for first because operators of a
//! self-hosted service spend their day in docker-compose, and asking them to
//! switch formats to configure one more thing is friction for nothing.
//!
//! Every value is validated in [`Config::load`]. A bad log filter or an
//! unsupported database URL therefore fails during start-up, not on the first
//! request that happens to touch it.

use std::{
    collections::BTreeMap,
    net::IpAddr,
    path::{Path, PathBuf},
    time::Duration,
};

use figment::{
    Figment,
    providers::{Env, Format, Serialized, Toml, Yaml},
};
use serde::{Deserialize, Serialize};
use tessera_core::{PasswordPolicy, RetrievalCode};

use crate::auth::cookie::SecurePolicy;

/// Prefix for every environment variable that feeds configuration.
pub const ENV_PREFIX: &str = "TESSERA_";

/// Separator used to express nesting in environment variables,
/// e.g. `TESSERA_SERVER__PORT=8080`.
pub const ENV_NESTED_SEPARATOR: &str = "__";

/// Environment variable naming the TOML file to read, if any.
pub const ENV_CONFIG_FILE: &str = "TESSERA_CONFIG_FILE";

/// Files looked for, in order, when [`ENV_CONFIG_FILE`] is unset.
///
/// The first that exists wins; they are not merged, because a value silently
/// coming from the second of two files is a debugging session nobody enjoys.
/// Absence of all of them is not an error — the environment alone is a complete
/// way to configure the service, and is what docker-compose uses.
pub const CONFIG_FILE_CANDIDATES: [&str; 3] = ["tessera.yaml", "tessera.yml", "tessera.toml"];

/// File written on first run when none of [`CONFIG_FILE_CANDIDATES`] exists.
pub const GENERATED_CONFIG_FILE: &str = "tessera.yaml";

/// Preamble of the generated template. Explains why nothing in it is active.
const TEMPLATE_HEADER: &str = "\
# tessera configuration
#
# Every line below is commented out on purpose. These are the values the binary
# is using right now, shown for reference — leaving them commented means you keep
# tracking the defaults, including improvements in later releases. Uncomment a
# line only when you want to pin it.
#
# Every setting can also be given as an environment variable: prefix TESSERA_ and
# separate nesting with __, e.g. server.port -> TESSERA_SERVER__PORT=9000.
# The environment wins over this file.
#
# This file is optional. Deleting it changes nothing.

";

/// Variables that share the [`ENV_PREFIX`] namespace without being settings.
///
/// [`Config`] is `deny_unknown_fields`, so a typo like `TESSERA_SERVER__PROT`
/// stops the process instead of being silently ignored — which is the behaviour
/// we want. The cost is that anything else in the namespace is read as a setting
/// too, and both of these would otherwise abort start-up:
///
/// - `CONFIG_FILE` selects the file to read, so it cannot be a field *in* it.
/// - `BUILD_REV` is consumed by `build.rs` at compile time; cargo re-exports it to the process it
///   builds, which broke every `cargo run` until this list existed.
///
/// Keep it minimal: every entry is a name a user can no longer typo-check.
const NON_CONFIG_ENV_KEYS: &[&str] = &["CONFIG_FILE", "BUILD_REV"];

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
    /// Which parts of the service exist on this instance.
    pub features: FeatureConfig,
    /// Account and session settings.
    pub auth: AuthConfig,
    /// Share creation and retrieval settings.
    pub share: ShareConfig,
    /// Logging and tracing settings.
    pub telemetry: TelemetryConfig,
}

/// Which parts of the service this instance exposes.
///
/// These are *runtime* switches: a disabled feature's routes are never mounted,
/// so calling one is a plain 404 rather than a 403 that confirms it exists.
/// Turning something off does not make the binary smaller — for that there are
/// cargo features, which gate whole backends (`postgres`, `storage-s3`) where
/// the saving is measured in megabytes rather than kilobytes.
///
/// `GET /api/v1/capabilities` reports the effective answer, because a
/// single-page app cannot know from a static contract which of these an
/// instance turned on.
// `struct_excessive_bools` suggests modelling this as a state machine. That is
// good advice when the flags are really one value in disguise, and wrong here:
// these switches are independent by design — the whole point of separating
// `accounts` from `anonymous_upload` is that every combination is a real
// deployment somebody wants.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches, not a state machine; see the comment above"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConfig {
    /// Registration, login and sessions.
    ///
    /// Off means the instance has no accounts at all — the right shape for one
    /// person behind a VPN who wants a file drop and nothing else.
    pub accounts: bool,

    /// Administrative endpoints, used by the separate admin front-end.
    pub admin: bool,

    /// Whether a caller without a session may upload.
    ///
    /// Deliberately independent of [`accounts`](Self::accounts): wanting an
    /// admin panel is not the same as wanting to force everyone to register,
    /// and coupling the two is how self-hosted tools end up unusable for the
    /// case they were built for.
    pub anonymous_upload: bool,

    /// The browsable API reference at `/docs`.
    ///
    /// Off by default: a development convenience rather than part of the
    /// product.
    ///
    /// The page this serves is a few lines of HTML that pull the renderer from a
    /// CDN. **The server never fetches it** — the browser does, so it is the
    /// reader's network that matters, not the instance's. A server behind a
    /// firewall with no outbound access still serves working docs to an operator
    /// on an ordinary connection, which is the common homelab shape. Only a
    /// genuinely air-gapped reader gets a blank page.
    ///
    /// Keeping the renderer remote is what holds ~11 MB of bundled UI out of the
    /// binary, measured: 23.8 MB with it embedded, 12.5 MB without.
    ///
    /// `openapi.json` is served either way — turning this off is about not
    /// shipping a UI, not about hiding the API.
    pub docs: bool,
}

/// Who may create an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationPolicy {
    /// Anyone may register.
    Open,
    /// Only with an invitation.
    Invite,
    /// Nobody; accounts are created by an administrator.
    Closed,
}

/// Account and session settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// Who may create an account.
    pub registration: RegistrationPolicy,

    /// How long a browser session stays valid, in hours.
    pub session_ttl_hours: u64,

    /// Password length policy. Composition rules are deliberately absent — see
    /// [`PasswordPolicy`].
    pub password: PasswordPolicy,

    /// When to mark the session cookie `Secure`.
    pub cookie_secure: SecurePolicy,

    /// Name for the administrator created on first run.
    pub admin_username: String,

    /// Address for the administrator created on first run.
    pub admin_email: String,

    /// Password for the administrator created on first run.
    ///
    /// Empty means "generate one and print it once". That is the friendlier
    /// default *and* the safer one: a password written into a compose file or an
    /// environment variable tends to stay there, and stay unchanged.
    pub admin_password: String,
}

/// How shares are created and handed out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareConfig {
    /// Characters in a generated retrieval code.
    ///
    /// This is the one figure an operator has a real reason to change, because
    /// the trade is theirs to make: a short code is quicker to read out loud, a
    /// long one is not worth scanning for. Eight Crockford base32 characters is
    /// about 1.1 x 10^12 combinations. Five — the figure some comparable
    /// services use — is 33 million, which a public endpoint can be walked
    /// through; that is how these services leak other people's uploads.
    ///
    /// Short codes are only defensible behind a throttle on failed lookups.
    pub code_length: usize,

    /// Content at or below this size is kept in the database rather than the
    /// blob store.
    ///
    /// Most shares on a personal instance are a short note or a snippet, and for
    /// those a round trip to object storage costs more than it saves — a backup
    /// of the database is also a backup of the content. Above it, bytes belong
    /// in the store, where they can be streamed rather than loaded.
    pub inline_threshold_bytes: usize,

    /// How long a download authorisation stays usable, in hours.
    ///
    /// One authorisation spends one of a share's permitted retrievals; the
    /// resulting ticket may then be presented as often as the transfer needs —
    /// for `Range` requests, for a resumed download, for a retry after the
    /// connection dropped. Without that separation a single interrupted
    /// download would burn a one-time share and the sender would have to upload
    /// it again.
    ///
    /// Long on purpose: ten gigabytes over a slow link takes most of a day, and
    /// whoever holds the ticket already has the right to the content.
    pub ticket_ttl_hours: u64,

    /// Longest lifetime a share may be given, in hours. `0` means no ceiling.
    pub max_expiry_hours: u64,
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

    /// Whether a reverse proxy in front of this service may be believed.
    ///
    /// Turns on trust in `X-Forwarded-Proto`, which decides whether the session
    /// cookie is marked `Secure`. Off by default: the header is set by whoever
    /// spoke to us last, so on a direct connection that is the client, and
    /// believing it would let anyone claim HTTPS.
    pub trusted_proxy: bool,
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
            trusted_proxy: false,
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

impl Default for FeatureConfig {
    fn default() -> Self {
        // Everything on. The shipped image is meant to work out of the box; an
        // operator who wants less says so, rather than having to discover which
        // switch was off by default.
        Self {
            accounts: true,
            admin: true,
            anonymous_upload: true,
            docs: false,
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            // Open by default because the instance belongs to whoever deployed
            // it, and the common case is a person setting it up for people they
            // know. An instance facing the open internet should close it.
            registration: RegistrationPolicy::Open,
            // 30 days. Long, on purpose: this is a tool people reach for
            // occasionally, and being logged out between uses is the kind of
            // friction that makes a self-hosted service go unused.
            session_ttl_hours: 24 * 30,
            password: PasswordPolicy::default(),
            cookie_secure: SecurePolicy::default(),
            admin_username: "admin".to_owned(),
            admin_email: "admin@localhost.invalid".to_owned(),
            admin_password: String::new(),
        }
    }
}

impl Default for ShareConfig {
    fn default() -> Self {
        Self {
            code_length: 8,
            inline_threshold_bytes: 64 * 1024,
            ticket_ttl_hours: 24,
            // 30 days. Not a limit anyone has asked for, but an instance that
            // never forgets anything eventually runs out of disk, and finding
            // that out is worse than being told at creation time.
            max_expiry_hours: 24 * 30,
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

impl AuthConfig {
    /// Session lifetime as a [`Duration`].
    #[must_use]
    pub const fn session_ttl(&self) -> Duration {
        Duration::from_secs(self.session_ttl_hours.saturating_mul(3600))
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
    /// Loads and validates configuration from defaults, an optional config file
    /// and the environment.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if a source cannot be parsed, if a value is
    /// unusable, or if the configuration selects a backend this binary was not
    /// compiled with.
    pub fn load() -> Result<Self, ConfigError> {
        match std::env::var(ENV_CONFIG_FILE) {
            // An explicit path is a statement of intent, so it is used whether or
            // not it exists — a typo there should surface as "no settings came
            // from the file" rather than being papered over by the candidates.
            Ok(path) => Self::load_from(Path::new(&path)),
            Err(_) => Self::load_from(&Self::discover_file()),
        }
    }

    /// The config file this process would read.
    ///
    /// The first of [`CONFIG_FILE_CANDIDATES`] that exists, falling back to the
    /// first candidate so callers always have a path to talk about.
    #[must_use]
    pub fn discover_file() -> PathBuf {
        CONFIG_FILE_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| PathBuf::from(CONFIG_FILE_CANDIDATES[0]))
    }

    /// Same as [`Config::load`], but reads the file at `path`.
    ///
    /// A missing file is not an error — it simply contributes nothing, which is
    /// what makes an environment-only deployment work.
    ///
    /// # Errors
    ///
    /// See [`Config::load`].
    pub fn load_from(path: &Path) -> Result<Self, ConfigError> {
        let figment = Figment::from(Serialized::defaults(Self::default()));

        // Chosen by extension rather than by sniffing the contents: a file named
        // `.yaml` that fails to parse should say so, not be quietly retried as
        // TOML and produce a confusing second error.
        let figment = if path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
        {
            figment.merge(Toml::file(path))
        } else {
            figment.merge(Yaml::file(path))
        };

        let config: Self = figment
            .merge(
                Env::prefixed(ENV_PREFIX)
                    .split(ENV_NESTED_SEPARATOR)
                    .ignore(NON_CONFIG_ENV_KEYS),
            )
            .extract()
            .map_err(Box::new)?;
        config.validate()?;
        Ok(config)
    }

    /// Writes a fully commented-out config file, if none exists yet.
    ///
    /// Returns the path written, or `None` if a file was already there.
    ///
    /// # Why every line is commented out
    ///
    /// The obvious version of this feature writes the current defaults as live
    /// settings. That freezes them: an operator who never touched
    /// `max_body_bytes` still has last year's value pinned in their file, and a
    /// later release that improves the default silently does nothing for them.
    ///
    /// Commented out, the file is documentation — it shows every option and what
    /// it is currently set to — while the values keep coming from the binary and
    /// keep improving with it. Uncommenting a line is then an explicit decision
    /// to stop tracking the default.
    ///
    /// The body is generated from [`Config::default`] rather than hand-written,
    /// so it cannot drift from the struct.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] only if the defaults cannot be serialised, which
    /// would be a bug. **Failing to write is not an error**: a read-only
    /// container filesystem is a perfectly good way to run this service, and the
    /// environment alone configures it completely.
    pub fn write_template_if_absent(path: &Path) -> Result<Option<PathBuf>, ConfigError> {
        if path.exists() {
            return Ok(None);
        }

        let body =
            serde_yaml::to_string(&Self::default()).map_err(|error| ConfigError::Invalid {
                field: "config",
                reason: format!("could not render the default configuration: {error}"),
            })?;

        let mut out = String::with_capacity(body.len() * 2);
        out.push_str(TEMPLATE_HEADER);
        for line in body.lines() {
            if line.trim().is_empty() {
                out.push('\n');
            } else {
                out.push_str("# ");
                out.push_str(line);
                out.push('\n');
            }
        }

        Ok(std::fs::write(path, out).ok().map(|()| path.to_path_buf()))
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
        self.features.validate()?;
        self.auth.validate()?;
        self.share.validate()?;
        self.telemetry.validate()
    }
}

impl ShareConfig {
    /// How long a download ticket lasts.
    #[must_use]
    pub const fn ticket_ttl(&self) -> Duration {
        Duration::from_secs(self.ticket_ttl_hours.saturating_mul(3600))
    }

    /// Longest lifetime a share may be given, or `None` when unbounded.
    #[must_use]
    pub const fn max_expiry(&self) -> Option<Duration> {
        if self.max_expiry_hours == 0 {
            None
        } else {
            Some(Duration::from_secs(
                self.max_expiry_hours.saturating_mul(3600),
            ))
        }
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if !(RetrievalCode::MIN_LENGTH..=RetrievalCode::MAX_LENGTH).contains(&self.code_length) {
            return Err(ConfigError::Invalid {
                field: "share.code_length",
                reason: format!(
                    "must be between {} and {}",
                    RetrievalCode::MIN_LENGTH,
                    RetrievalCode::MAX_LENGTH
                ),
            });
        }
        if self.inline_threshold_bytes > INLINE_CEILING {
            return Err(ConfigError::Invalid {
                field: "share.inline_threshold_bytes",
                reason: format!(
                    "must be at most {INLINE_CEILING}; larger content belongs in the blob \
                     store, where it can be streamed rather than held in memory"
                ),
            });
        }
        if self.ticket_ttl_hours == 0 {
            return Err(ConfigError::Invalid {
                field: "share.ticket_ttl_hours",
                reason: "must be at least 1; a ticket that expires immediately makes every \
                         download spend a retrieval"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

/// Highest inline threshold an operator may configure.
///
/// Inline content is read into memory whole, so this is the largest allocation a
/// single retrieval can be made to perform. MySQL's `MEDIUMBLOB` would hold far
/// more; the limit here is about memory, not about the column.
const INLINE_CEILING: usize = 1024 * 1024;

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

impl FeatureConfig {
    fn validate(self) -> Result<(), ConfigError> {
        // An instance with neither accounts nor anonymous upload can receive
        // nothing at all. That is never what anybody meant, and it is far kinder
        // to say so at start-up than to let every upload fail with a 403.
        if !self.accounts && !self.anonymous_upload {
            return Err(ConfigError::Invalid {
                field: "features.anonymous_upload",
                reason: "must be enabled when `features.accounts` is off, or nothing could \
                         ever be uploaded"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

impl AuthConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if self.session_ttl_hours == 0 {
            return Err(ConfigError::Invalid {
                field: "auth.session_ttl_hours",
                reason: "must be at least 1 hour".to_owned(),
            });
        }
        // The policy's own bounds live in the domain crate, so the rule is the
        // same whether it arrives from a file or from a test.
        self.password
            .validate()
            .map_err(|error| ConfigError::Invalid {
                field: "auth.password",
                reason: error.to_string(),
            })
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
            let config = Config::load_from(Path::new("/nonexistent/tessera.yaml"))
                .expect("no file, no error");
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
    #[case::short_code("share.code_length", |c: &mut Config| c.share.code_length = 2)]
    #[case::long_code("share.code_length", |c: &mut Config| c.share.code_length = 64)]
    #[case::huge_inline("share.inline_threshold_bytes", |c: &mut Config| {
        c.share.inline_threshold_bytes = 8 * 1024 * 1024;
    })]
    #[case::zero_ticket("share.ticket_ttl_hours", |c: &mut Config| c.share.ticket_ttl_hours = 0)]
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
            jail.set_env("TESSERA_FEATURES__DOCS", "false");
            jail.set_env("TESSERA_DATABASE__URL", "sqlite://custom.db");

            let config = Config::load_from(Path::new("tessera.toml")).expect("valid configuration");

            // Environment beats file...
            assert_eq!(config.server.port, 9999);
            assert!(!config.features.docs);
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
            let err = Config::load_from(Path::new("tessera.toml")).expect_err("must be rejected");
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

    /// Regression test. `TESSERA_CONFIG_FILE` is the documented way to point at
    /// a config file, and `TESSERA_BUILD_REV` is exported into every `cargo run`
    /// by the build script. Both sit inside the settings namespace, so
    /// `deny_unknown_fields` rejected them and the process refused to start.
    #[rstest]
    #[case("TESSERA_CONFIG_FILE", "/etc/tessera.toml")]
    #[case("TESSERA_BUILD_REV", "v0.1.0-3-gabcdef")]
    fn namespace_neighbours_are_not_mistaken_for_settings(#[case] key: &str, #[case] value: &str) {
        figment::Jail::expect_with(|jail| {
            jail.set_env(key, value);
            let config = Config::load_from(Path::new("tessera.toml"))
                .unwrap_or_else(|e| panic!("{key} must not be read as a setting: {e}"));
            assert_eq!(config, Config::default());
            Ok(())
        });
    }

    /// The property the whole feature rests on: the generated file must contain
    /// no *active* settings. If it did, an operator's defaults would be frozen at
    /// whatever version first started the service.
    #[test]
    fn the_generated_template_is_entirely_commented_out() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tessera.yaml");

        let written = Config::write_template_if_absent(&path).unwrap();
        assert_eq!(written.as_deref(), Some(path.as_path()));

        let text = std::fs::read_to_string(&path).unwrap();
        for (number, line) in text.lines().enumerate() {
            assert!(
                line.trim().is_empty() || line.trim_start().starts_with('#'),
                "line {} is active: {line}",
                number + 1
            );
        }

        // And it must really be a config file, not an empty one.
        assert!(
            text.contains("port"),
            "the template should list the settings"
        );
        assert!(text.contains("accounts"));
    }

    /// Loading the generated file must change nothing — it is documentation.
    #[test]
    fn the_generated_template_parses_and_changes_nothing() {
        figment::Jail::expect_with(|jail| {
            let path = jail.directory().join("tessera.yaml");
            Config::write_template_if_absent(&path).expect("template");
            let config = Config::load_from(&path).expect("the template must be loadable");
            assert_eq!(config, Config::default());
            Ok(())
        });
    }

    #[test]
    fn an_existing_config_file_is_never_overwritten() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("tessera.yaml");
        std::fs::write(&path, "server:\n  port: 9999\n").unwrap();

        assert_eq!(Config::write_template_if_absent(&path).unwrap(), None);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "server:\n  port: 9999\n"
        );
    }

    /// A read-only filesystem is a legitimate way to run this service.
    #[test]
    fn failing_to_write_the_template_is_not_an_error() {
        let path = Path::new("/proc/definitely/not/writable/tessera.yaml");
        assert_eq!(Config::write_template_if_absent(path).unwrap(), None);
    }

    #[test]
    fn yaml_and_toml_are_both_understood() {
        figment::Jail::expect_with(|jail| {
            jail.create_file("tessera.yaml", "server:\n  port: 9101\n")?;
            let from_yaml = Config::load_from(Path::new("tessera.yaml")).expect("yaml");
            assert_eq!(from_yaml.server.port, 9101);

            jail.create_file("tessera.toml", "[server]\nport = 9102\n")?;
            let from_toml = Config::load_from(Path::new("tessera.toml")).expect("toml");
            assert_eq!(from_toml.server.port, 9102);
            Ok(())
        });
    }

    /// An instance that accepts neither anonymous uploads nor accounts can
    /// receive nothing at all, which is never what anybody configured on purpose.
    #[test]
    fn rejects_an_instance_that_could_never_receive_anything() {
        let mut config = Config::default();
        config.features.accounts = false;
        config.features.anonymous_upload = false;

        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("anonymous_upload"), "got: {err}");
    }

    #[test]
    fn accounts_off_with_anonymous_upload_on_is_a_valid_instance() {
        let mut config = Config::default();
        config.features.accounts = false;
        config.features.anonymous_upload = true;
        config.validate().unwrap();
    }

    #[test]
    fn rejects_a_password_policy_the_domain_would_not_accept() {
        let mut config = Config::default();
        config.auth.password.min_length = 4;
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("auth.password"), "got: {err}");
    }

    #[test]
    fn unknown_keys_are_rejected_rather_than_ignored() {
        figment::Jail::expect_with(|jail| {
            // A typo in a config file must not silently do nothing.
            jail.create_file("tessera.toml", "[server]\nprot = 9001\n")?;
            let err = Config::load_from(Path::new("tessera.toml")).expect_err("must be rejected");
            assert!(matches!(err, ConfigError::Source(_)), "got {err}");
            Ok(())
        });
    }

    /// A code length the domain type would refuse is one every generated code
    /// fails on — better to say so at start-up than on the first upload.
    #[test]
    fn the_default_code_length_is_one_the_domain_accepts() {
        let config = ShareConfig::default();
        let entropy = vec![0_u8; config.code_length];
        assert!(
            RetrievalCode::from_entropy(&entropy).is_ok(),
            "share.code_length must be usable by RetrievalCode"
        );
    }

    /// `0` is the documented way to say "no ceiling", so it must not be rejected
    /// alongside the other zeroes.
    #[test]
    fn an_unbounded_expiry_is_allowed() {
        let config = ShareConfig {
            max_expiry_hours: 0,
            ..ShareConfig::default()
        };
        assert!(config.validate().is_ok());
        assert!(config.max_expiry().is_none());
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
