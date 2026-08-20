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
    /// Outgoing mail. Empty `host` means this instance sends nothing.
    pub mail: MailConfig,
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

/// Where outgoing mail goes, when there is any.
///
/// # Empty means off, not broken
///
/// An empty `host` is the default and the common case: the instance this was
/// built for has one user, and the way back into a forgotten account is
/// `reset-password` on the box itself. Nothing is mounted, nothing is
/// advertised, and start-up says nothing — an instance that does not send mail
/// is not missing a setting.
///
/// # Why not a mail server
///
/// Nobody should run one for this. Home connections cannot deliver mail:
/// port 25 is blocked by most providers and receiving servers reject
/// residential addresses regardless of SPF and DKIM. What this expects is a
/// relay — Resend, Brevo, SES, Fastmail, or a company's own — which is four
/// values pasted into a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailConfig {
    /// SMTP host. Empty turns off everything that sends mail.
    pub host: String,

    /// SMTP port. 587 is submission with STARTTLS, which is what relays want;
    /// 465 is implicit TLS and also common.
    pub port: u16,

    /// SMTP username. Resend wants the literal `resend` here.
    pub username: String,

    /// SMTP password. For most relays this is an API key.
    pub password: String,

    /// The `From:` address. Must be one the relay will accept — a domain you
    /// have verified with them, not an arbitrary address.
    pub from: String,

    /// Where this instance is reachable, used to build the links in mail.
    ///
    /// Configured rather than taken from the request's `Host` header, and that
    /// is a security property rather than a convenience: a reset link built
    /// from a header an attacker controls would point at *their* server, and
    /// the person clicking it would hand over their token.
    pub public_url: String,

    /// How long a password reset link works for, in minutes.
    ///
    /// Short because the link *is* the credential. Long enough to survive a
    /// mail queue and somebody reading their inbox after lunch, and capped at
    /// [`MAX_RESET_TTL_MINUTES`] because past that it stops being a reset and
    /// becomes a spare key living in a mailbox.
    pub reset_ttl_minutes: u64,

    /// How long a registration confirmation link works for, in hours.
    ///
    /// Much longer than a reset link, because the two are asked for under
    /// different circumstances. A reset is somebody standing at a locked door;
    /// a registration confirmation often sits in an inbox until the evening.
    /// The link is also worth less: following it creates an account with a
    /// password whoever asked already chose, so a stolen one gains an account
    /// its thief cannot sign into.
    pub registration_ttl_hours: u64,
}

/// The longest a reset link may be configured to last, in minutes.
///
/// A day. Long enough for any mail delay worth surviving; past it the link has
/// stopped being a way back in and become a second credential, sitting
/// somewhere far less protected than a password.
pub const MAX_RESET_TTL_MINUTES: u64 = 24 * 60;

/// The longest a registration link may be configured to last, in hours.
///
/// A week. The row behind it holds a password hash for an account that does not
/// exist, and keeping one of those indefinitely is a liability with no
/// corresponding use — nobody confirms a registration a fortnight later.
pub const MAX_REGISTRATION_TTL_HOURS: u64 = 7 * 24;

impl Default for MailConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 587,
            username: String::new(),
            password: String::new(),
            from: String::new(),
            public_url: String::new(),
            reset_ttl_minutes: 30,
            registration_ttl_hours: 24,
        }
    }
}

impl MailConfig {
    /// Whether this instance can send mail at all.
    #[must_use]
    pub const fn is_configured(&self) -> bool {
        !self.host.is_empty() && !self.from.is_empty() && !self.public_url.is_empty()
    }

    /// How long a registration confirmation link is good for.
    ///
    /// Saturating for the same reason as [`MailConfig::reset_ttl`].
    #[must_use]
    pub const fn registration_ttl(&self) -> Duration {
        Duration::from_secs(self.registration_ttl_hours.saturating_mul(60 * 60))
    }

    /// How long a reset link is good for.
    ///
    /// Saturating rather than plain multiplication: validation caps the value,
    /// but this is `pub` and a caller that built a `MailConfig` by hand should
    /// get a long duration rather than a panic in debug and a wrapped one in
    /// release.
    #[must_use]
    pub const fn reset_ttl(&self) -> Duration {
        Duration::from_secs(self.reset_ttl_minutes.saturating_mul(60))
    }
}

/// Account and session settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    /// Who may create an account.
    pub registration: RegistrationPolicy,

    /// How long a browser session stays valid, in hours.
    pub session_ttl_hours: u64,
    /// How long an ended session stays in the list of logins, in days.
    ///
    /// The rows are what a person reads to answer "has anybody else been in
    /// here", so they outlive the credential. Long enough to still answer that
    /// about last month, short enough that a home instance is not carrying a
    /// year of them.
    #[serde(default = "default_login_history_days")]
    pub login_history_days: u64,

    /// Password length policy. Composition rules are deliberately absent — see
    /// [`PasswordPolicy`].
    pub password: PasswordPolicy,

    /// When to mark the session cookie `Secure`.
    pub cookie_secure: SecurePolicy,

    /// Which role a newly registered account goes into.
    ///
    /// By name, because that is what an operator reads in the admin panel.
    ///
    /// # Why this rather than editing the built-in role
    ///
    /// "I want ordinary users to be able to do less" is the real need behind
    /// wanting to edit `member`, and it is better served by choosing a
    /// different role than by changing what `member` means — a built-in role
    /// that means something different on every instance is one that
    /// documentation and support conversations cannot rely on.
    ///
    /// The role has to exist. That is checked at startup rather than at
    /// somebody's registration, because the failure would otherwise arrive as
    /// a stranger's `500`.
    #[serde(default = "default_role")]
    pub default_role: String,

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
    /// The largest a text share may be, in bytes.
    ///
    /// Text lives in the database, so this is the size of the largest row this
    /// service will write and the most memory one upload can hold at once —
    /// there is no streaming a value that has to be handed over whole.
    ///
    /// A megabyte is about a million characters. The whole of *The Lord of the
    /// Rings* as plain text is around two and a half, so a paste that does not
    /// fit here is not a paste. Somebody who wants to share something larger
    /// sends it as a file, which is streamed to the object store and never held
    /// in full.
    pub max_text_bytes: usize,

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

    /// Lifetime a share gets when the request does not ask for one, in hours.
    ///
    /// This is the figure that decides how much disk an instance holds, and it
    /// decides it more than any quota does. Ephemeral storage does not
    /// accumulate: what is on disk at any moment is roughly the upload rate
    /// multiplied by how long things are kept, so an instance keeping shares for
    /// a week holds a week of uploads however long it runs.
    ///
    /// A week rather than a day because the common case is handing a link to
    /// somebody who is asleep, or in another timezone, or who will not read
    /// their messages until Monday. Rather than a month because a default is
    /// what most shares get, and most shares are collected within hours.
    ///
    /// `0` means "use [`max_expiry_hours`](Self::max_expiry_hours)", which is
    /// what this used to do unconditionally.
    pub default_expiry_hours: u64,

    /// Longest lifetime a share may be given, in hours. `0` means no ceiling.
    pub max_expiry_hours: u64,

    /// How long a finished share is kept before it is removed, in hours.
    ///
    /// A share that has expired or been used up is kept for a while so that
    /// following its link still says "this expired" rather than "no such code" —
    /// which are different things to tell a person, and only one of them is
    /// worth retyping. After this, the row and whatever it put in the blob store
    /// both go.
    ///
    /// `0` removes them on the next sweep.
    pub retain_after_death_hours: u64,

    /// How long an unfinished upload waits before the sweep removes it.
    ///
    /// It is holding real bytes in the store the whole time, and they count
    /// against the quota — so this is how long somebody may reserve space by
    /// starting an upload and walking away. Long enough that a laptop closed
    /// overnight can still resume; short enough that abandoned ones do not
    /// accumulate.
    pub upload_session_ttl_hours: u64,
}

use crate::auth::proxy::TrustedProxies;

/// HTTP listener and middleware settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    /// Address to bind. `0.0.0.0` inside containers, `127.0.0.1` behind a proxy.
    pub host: IpAddr,
    /// TCP port to bind.
    pub port: u16,
    /// Per-request wall-clock budget, in seconds.
    ///
    /// Does not apply to upload routes — see `upload_stall_secs` for why they
    /// need a different question asked of them.
    pub request_timeout_secs: u64,

    /// How long an upload may go without a byte arriving, in seconds.
    ///
    /// # Why uploads are not given a wall-clock budget
    ///
    /// One number cannot be both. To let a 5 GB upload finish over a 20 Mbit/s
    /// line it would have to be 2400 seconds, and that is also how long a client
    /// that connects and then sends nothing would hold a slot. The two demands
    /// are three orders of magnitude apart, so any single value is wrong for one
    /// of them.
    ///
    /// This asks the question that separates them: is the other end still
    /// sending? A 5 GB upload passes for as long as it takes, because every
    /// second of it moves bytes; a dead connection is dropped in this many.
    pub upload_stall_secs: u64,
    /// Maximum accepted request body, in bytes.
    pub max_body_bytes: usize,

    /// How much of an upload is gathered before a part goes to the store.
    ///
    /// Only reaches an object store that does multipart. S3 refuses every part
    /// but the last below 5 MiB, and Cloudflare R2 additionally requires every
    /// part but the last to be the *same* size — so an instance on R2 wants
    /// this and the minimum to agree, which is what tusd does. Garage enforces
    /// neither, which is why the CI job cannot catch a mistake here.
    pub part_size_bytes: usize,

    /// Directory to serve the frontend from, instead of the bundle compiled in.
    ///
    /// Empty, the default, serves the compiled-in bundle — one file, no web
    /// server in front of it, which is the point of embedding it.
    ///
    /// Set, every request reads from disk, so rebuilding the frontend and
    /// refreshing the browser is the whole loop. That is what this exists for:
    /// without it, changing one line of CSS and seeing it in a container means
    /// waiting for a Rust build.
    ///
    /// A directory with no `index.html` in it stops the server rather than
    /// falling back. Falling back quietly would show somebody yesterday's build
    /// while they believed they were looking at today's.
    #[serde(default)]
    pub assets_dir: String,
    /// Origins allowed to call the API from a browser. Empty means same-origin
    /// only, which is the correct default for the single-binary deployment.
    pub cors_allowed_origins: Vec<String>,

    /// Which proxies in front of this service may be believed, as CIDR blocks
    /// or bare addresses.
    ///
    /// Empty by default, which means nothing is believed: `X-Forwarded-For` and
    /// `X-Forwarded-Proto` are set by whoever spoke to us last, so on a direct
    /// connection that is the caller. Believing them from anyone would let a
    /// caller pick their own source address — and every throttle in this service
    /// counts against that address — as well as claim HTTPS on a plain
    /// connection.
    ///
    /// `["127.0.0.1", "::1"]` covers a proxy on the same host;
    /// `["10.0.0.0/8"]` a container network.
    #[serde(default, deserialize_with = "list_or_commas")]
    pub trusted_proxies: Vec<String>,

    /// Deprecated: use [`ServerConfig::trusted_proxies`].
    ///
    /// `true` means "whatever connects to us is a proxy", which is the setting
    /// that makes the headers forgeable by anyone who can reach the port
    /// directly. Kept working so an upgrade does not lock an operator out of
    /// their own instance, and warned about at start-up.
    #[serde(default)]
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
    /// Most bytes of live share content this instance will hold. `0` is no
    /// limit.
    ///
    /// Without this, an instance with `anonymous_upload` on — which is the
    /// default — is an unlimited anonymous drive for anyone who can reach it,
    /// and the first sign of trouble is writes failing because the disk is
    /// full. That is the worst place to find out: the shares already on it are
    /// fine, and the ones being refused get no explanation.
    ///
    /// Counts content that still exists, wherever it lives — content under
    /// `share.inline_threshold_bytes` sits in the database rather than the blob
    /// store, and it is still disk. A share kept past its death so its link can
    /// say "this expired" has already had its bytes removed, in both places,
    /// and does not count against this.
    ///
    /// Approximate under concurrency: uploads in flight together can land past
    /// the line by up to their own number times `server.max_body_bytes`, and the
    /// next one is refused. It is a guard against unbounded growth, not an
    /// accounting system — leave the same slack you would leave on a disk.
    ///
    /// `share.default_expiry_hours` is the other half of the same question, and
    /// usually the more effective one: what is held is roughly the upload rate
    /// times how long things are kept, so shortening the default lowers the
    /// steady state without ever refusing anybody.
    pub max_total_bytes: u64,

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
            upload_stall_secs: 30,
            // 100 MiB. Generous for a courier service default, and overridable.
            max_body_bytes: 100 * 1024 * 1024,
            // 8 MiB: above S3's 5 MiB floor with room to spare, and small
            // enough that a part which has to be retried is not a long retry.
            part_size_bytes: 8 * 1024 * 1024,
            assets_dir: String::new(),
            cors_allowed_origins: Vec::new(),
            trusted_proxies: Vec::new(),
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
            // No limit. The common deployment is one person on hardware they
            // own, where a ceiling they did not ask for is an obstacle and the
            // disk is something they can already see. The deployment that needs
            // one is an instance strangers upload to, and that is not a default
            // anybody falls into by accident — but `Config::load` warns when it
            // sees the combination, so it is not a silent trap either.
            max_total_bytes: 0,
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
            default_role: default_role(),
            session_ttl_hours: 24 * 30,
            login_history_days: default_login_history_days(),
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
            max_text_bytes: 1024 * 1024,
            ticket_ttl_hours: 24,
            // A week. This is what almost every share gets, because almost no
            // caller asks for a lifetime — so it, and not the ceiling below,
            // is what decides how much disk an instance holds.
            default_expiry_hours: 24 * 7,
            // 30 days. Not a limit anyone has asked for, but an instance that
            // never forgets anything eventually runs out of disk, and finding
            // that out is worse than being told at creation time.
            //
            // Deliberately far above the default: this is the longest a person
            // may *ask* for, which is a different question from what they get
            // when they do not ask.
            max_expiry_hours: 24 * 30,
            // A week. Long enough that somebody coming back to a link they were
            // sent gets told what happened, short enough that dead payloads do
            // not become the reason the disk fills.
            upload_session_ttl_hours: 24,
            retain_after_death_hours: 24 * 7,
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

/// Reads a list written either as a list or as one comma-separated string.
///
/// YAML and TOML both have list syntax, so a config file has no need of this.
/// An environment variable does: `TESSERA_SERVER__TRUSTED_PROXIES` has to carry
/// its structure inside a string, and the JSON figment expects
/// (`["127.0.0.1","::1"]`) is not what anybody writes first. Refusing the
/// obvious spelling would be a small thing anywhere else — here it is the
/// setting that decides whether the throttles work, and the way out of a
/// setting somebody cannot get to load is the deprecated boolean that turns
/// them off.
fn list_or_commas<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Written {
        List(Vec<String>),
        Joined(String),
    }

    Ok(match Written::deserialize(deserializer)? {
        Written::List(entries) => entries,
        Written::Joined(line) => line
            .split(',')
            .map(str::trim)
            .filter(|entry| !entry.is_empty())
            .map(str::to_owned)
            .collect(),
    })
}

impl ServerConfig {
    /// Per-request timeout as a [`Duration`].
    #[must_use]
    pub const fn request_timeout(&self) -> Duration {
        Duration::from_secs(self.request_timeout_secs)
    }

    /// How long an upload may go without a byte, as a [`Duration`].
    #[must_use]
    pub const fn upload_stall(&self) -> Duration {
        Duration::from_secs(self.upload_stall_secs)
    }

    /// The proxies this instance will be spoken for by.
    ///
    /// A non-empty list wins over the deprecated boolean, so an operator part
    /// way through migrating gets the setting that says something specific
    /// rather than the one that says "anyone".
    ///
    /// The list is validated during [`Config::validate`], so by the time this
    /// is called it cannot fail — an unusable entry has already stopped
    /// start-up. Falling back to trusting nothing is the safe reading of a state
    /// that should not be reachable.
    #[must_use]
    pub fn proxy_trust(&self) -> TrustedProxies {
        if !self.trusted_proxies.is_empty() {
            return TrustedProxies::parse(&self.trusted_proxies).unwrap_or_default();
        }
        if self.trusted_proxy {
            return TrustedProxies::any_peer();
        }
        TrustedProxies::default()
    }
}

/// Days of login history kept, when nothing says otherwise.
const fn default_login_history_days() -> u64 {
    90
}

/// The role the migration seeds ordinary accounts into.
fn default_role() -> String {
    "member".to_owned()
}

/// The most history an instance may be asked to keep.
///
/// A bound rather than a preference: the sweep is what stops the table growing
/// without limit, and a value in the thousands turns it off without saying so.
const MAX_LOGIN_HISTORY_DAYS: u64 = 365 * 2;

impl AuthConfig {
    /// Session lifetime as a [`Duration`].
    #[must_use]
    pub const fn session_ttl(&self) -> Duration {
        Duration::from_secs(self.session_ttl_hours.saturating_mul(3600))
    }

    /// How long ended sessions are kept, as a [`Duration`].
    #[must_use]
    pub const fn login_history(&self) -> Duration {
        Duration::from_secs(self.login_history_days.saturating_mul(24 * 3600))
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
        self.telemetry.validate()?;
        self.mail.validate()
    }
}

impl MailConfig {
    /// Refuses a half-filled mail block.
    ///
    /// Being half-configured is worse than being empty: empty means the feature
    /// does not exist and every path around it is quiet, whereas a host with no
    /// `from` means the endpoints get mounted, the frontend draws the link, and
    /// the failure only shows up when somebody is already locked out.
    fn validate(&self) -> Result<(), ConfigError> {
        let given: Vec<&str> = [
            (!self.host.is_empty()).then_some("mail.host"),
            (!self.from.is_empty()).then_some("mail.from"),
            (!self.public_url.is_empty()).then_some("mail.public_url"),
        ]
        .into_iter()
        .flatten()
        .collect();

        if given.is_empty() || given.len() == 3 {
            // All or nothing, which is what `is_configured` also asks.
        } else {
            return Err(ConfigError::Invalid {
                field: "mail",
                reason: format!(
                    "set all of mail.host, mail.from and mail.public_url, or none of them                      (only {} given)",
                    given.join(", ")
                ),
            });
        }

        if !self.is_configured() {
            return Ok(());
        }
        if self.port == 0 {
            return Err(ConfigError::Invalid {
                field: "mail.port",
                reason: "must not be 0".to_owned(),
            });
        }
        if !self.public_url.starts_with("http://") && !self.public_url.starts_with("https://") {
            return Err(ConfigError::Invalid {
                field: "mail.public_url",
                reason: "must start with http:// or https:// — it is the base of a link somebody                          will click"
                    .to_owned(),
            });
        }
        if self.reset_ttl_minutes == 0 || self.reset_ttl_minutes > MAX_RESET_TTL_MINUTES {
            return Err(ConfigError::Invalid {
                field: "mail.reset_ttl_minutes",
                reason: format!(
                    "must be between 1 and {MAX_RESET_TTL_MINUTES}; a link that has already                      expired helps nobody, and one that lasts longer than a day is a spare key                      sitting in a mailbox"
                ),
            });
        }
        if self.registration_ttl_hours == 0
            || self.registration_ttl_hours > MAX_REGISTRATION_TTL_HOURS
        {
            return Err(ConfigError::Invalid {
                field: "mail.registration_ttl_hours",
                reason: format!(
                    "must be between 1 and {MAX_REGISTRATION_TTL_HOURS}; an unconfirmed \
                     registration holds a password hash for an account that does not exist, and \
                     keeping one for longer than a week serves nobody"
                ),
            });
        }
        Ok(())
    }
}

impl ShareConfig {
    /// How long a download ticket lasts.
    #[must_use]
    pub const fn ticket_ttl(&self) -> Duration {
        Duration::from_secs(self.ticket_ttl_hours.saturating_mul(3600))
    }

    /// How long a finished share is kept before removal.
    #[must_use]
    pub const fn retention(&self) -> Duration {
        Duration::from_secs(self.retain_after_death_hours.saturating_mul(3600))
    }

    /// How long an unfinished upload waits, as a [`Duration`].
    #[must_use]
    pub const fn upload_session_ttl(&self) -> Duration {
        Duration::from_secs(self.upload_session_ttl_hours.saturating_mul(3600))
    }

    /// Lifetime an unspecified share gets, or `None` when even that is
    /// unbounded.
    ///
    /// Falls back to the ceiling, which is what this did before the default was
    /// separable — and is still the only sensible reading of "no default" on an
    /// instance that has a ceiling.
    #[must_use]
    pub const fn default_expiry(&self) -> Option<Duration> {
        if self.default_expiry_hours == 0 {
            self.max_expiry()
        } else {
            Some(Duration::from_secs(
                self.default_expiry_hours.saturating_mul(3600),
            ))
        }
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
        if self.max_text_bytes == 0 || self.max_text_bytes > MAX_TEXT_CEILING {
            return Err(ConfigError::Invalid {
                field: "share.max_text_bytes",
                reason: format!(
                    "must be between 1 and {MAX_TEXT_CEILING}; text is held whole in memory \
                     while it is read and stored whole as one row, so a larger value turns one \
                     upload into that much resident memory and that much of a single read"
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
        // A default above the ceiling would make every unspecified share fail
        // the very check the ceiling exists for, and the caller could not tell
        // why: they did not ask for a lifetime at all.
        if self.max_expiry_hours != 0
            && self.default_expiry_hours != 0
            && self.default_expiry_hours > self.max_expiry_hours
        {
            return Err(ConfigError::Invalid {
                field: "share.default_expiry_hours",
                reason: format!(
                    "must not exceed share.max_expiry_hours ({})",
                    self.max_expiry_hours
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

/// The most a text share may ever be allowed to be.
///
/// A bound on the setting, not the setting: an operator who wants larger pastes
/// can raise `share.max_text_bytes`, but not to a number that makes one upload
/// a memory problem for everybody else on the instance. Sixteen megabytes is
/// already six times the plain text of a long novel.
const MAX_TEXT_CEILING: usize = 16 * 1024 * 1024;

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
        // Below S3's floor every multipart upload of more than one part fails
        // at `complete`, which is late, remote and hard to read.
        if self.part_size_bytes < 5 * 1024 * 1024 {
            return Err(ConfigError::Invalid {
                field: "server.part_size_bytes",
                reason: "must be at least 5 MiB, which is S3's minimum for every \
                         part but the last"
                    .to_owned(),
            });
        }
        if self.upload_stall_secs == 0 {
            return Err(ConfigError::Invalid {
                field: "server.upload_stall_secs",
                reason: "must be at least 1 second".to_owned(),
            });
        }
        if self.max_body_bytes == 0 {
            return Err(ConfigError::Invalid {
                field: "server.max_body_bytes",
                reason: "must be at least 1 byte".to_owned(),
            });
        }
        if let Err(entry) = TrustedProxies::parse(&self.trusted_proxies) {
            return Err(ConfigError::Invalid {
                field: "server.trusted_proxies",
                reason: format!(
                    "`{entry}` is not an address or a CIDR block; expected something like \
                     `127.0.0.1` or `10.0.0.0/8`"
                ),
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
        if self.login_history_days == 0 || self.login_history_days > MAX_LOGIN_HISTORY_DAYS {
            return Err(ConfigError::Invalid {
                field: "auth.login_history_days",
                reason: format!(
                    "must be between 1 and {MAX_LOGIN_HISTORY_DAYS}; the sweep is what keeps the \
                     table bounded, and a value beyond this turns it off without saying so"
                ),
            });
        }
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
    use axum::http::HeaderMap;

    /// The smallest mail block that passes the all-or-nothing rule, for cases
    /// that are about one of the *other* fields.
    fn configured_mail(config: &mut Config) {
        config.mail.host = "smtp.example.com".to_owned();
        config.mail.from = "tessera@example.com".to_owned();
        config.mail.public_url = "https://tessera.example.com".to_owned();
    }

    use rstest::rstest;

    use super::*;

    /// The default is what almost every share gets, because almost no caller
    /// asks for a lifetime. It used to *be* the ceiling, which meant the longest
    /// life anyone was allowed to request was also what everyone got without
    /// asking — thirty days of uploads held to serve links that are mostly
    /// collected within hours.
    #[test]
    fn the_default_lifetime_is_shorter_than_the_longest_allowed() {
        let share = ShareConfig::default();
        assert!(
            share.default_expiry() < share.max_expiry(),
            "default {:?} should be well under the ceiling {:?}",
            share.default_expiry(),
            share.max_expiry()
        );
        assert_eq!(share.default_expiry_hours, 24 * 7, "a week");
    }

    /// `0` is how an operator says "I have no opinion", and the only sensible
    /// reading of that on an instance with a ceiling is the ceiling — which is
    /// also what this did before the two were separable.
    #[test]
    fn a_zero_default_falls_back_to_the_ceiling() {
        let share = ShareConfig {
            default_expiry_hours: 0,
            ..ShareConfig::default()
        };
        assert_eq!(share.default_expiry(), share.max_expiry());

        let unbounded = ShareConfig {
            default_expiry_hours: 0,
            max_expiry_hours: 0,
            ..ShareConfig::default()
        };
        assert_eq!(unbounded.default_expiry(), None);
    }

    /// A default above the ceiling would make every share that asks for nothing
    /// fail the check the ceiling exists for, and the caller could not act on
    /// the error — they did not request a lifetime at all.
    #[test]
    fn a_default_above_the_ceiling_is_refused() {
        let share = ShareConfig {
            default_expiry_hours: 24 * 60,
            max_expiry_hours: 24 * 30,
            ..ShareConfig::default()
        };
        assert!(share.validate().is_err());

        // No ceiling means nothing to exceed.
        let unbounded = ShareConfig {
            default_expiry_hours: 24 * 60,
            max_expiry_hours: 0,
            ..ShareConfig::default()
        };
        assert!(unbounded.validate().is_ok());
    }

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
    #[case::small_part("server.part_size_bytes", |c: &mut Config| c.server.part_size_bytes = 1024)]
    #[case::zero_stall("server.upload_stall_secs", |c: &mut Config| c.server.upload_stall_secs = 0)]
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
    // Half a mail block is worse than none: empty means the feature does not
    // exist and every path around it is quiet, whereas a host with no `from`
    // mounts the endpoints and draws the link, and only fails once somebody is
    // already locked out.
    #[case::half_configured_mail("mail", |c: &mut Config| {
        c.mail.host = "smtp.example.com".to_owned();
    })]
    #[case::mail_without_public_url("mail", |c: &mut Config| {
        c.mail.host = "smtp.example.com".to_owned();
        c.mail.from = "t@example.com".to_owned();
    })]
    #[case::mail_public_url_without_scheme("mail.public_url", |c: &mut Config| {
        configured_mail(c);
        c.mail.public_url = "tessera.example.com".to_owned();
    })]
    #[case::zero_reset_ttl("mail.reset_ttl_minutes", |c: &mut Config| {
        configured_mail(c);
        c.mail.reset_ttl_minutes = 0;
    })]
    // A link that outlives the day has stopped being a way back in and become a
    // second credential, kept somewhere far less protected than a password.
    #[case::endless_reset_ttl("mail.reset_ttl_minutes", |c: &mut Config| {
        configured_mail(c);
        c.mail.reset_ttl_minutes = MAX_RESET_TTL_MINUTES + 1;
    })]
    // The multiplication behind this used to be plain, so a value near the
    // ceiling of the type panicked in debug and wrapped in release.
    #[case::absurd_reset_ttl("mail.reset_ttl_minutes", |c: &mut Config| {
        configured_mail(c);
        c.mail.reset_ttl_minutes = u64::MAX;
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

    /// A fully configured mail block is accepted, and so is an empty one — the
    /// two ends of the all-or-nothing rule.
    #[test]
    fn mail_is_accepted_when_whole_and_when_absent() {
        let mut config = Config::default();
        assert!(
            config.validate().is_ok(),
            "the default sends no mail at all"
        );
        assert!(!config.mail.is_configured());

        configured_mail(&mut config);
        assert!(config.validate().is_ok());
        assert!(config.mail.is_configured());
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
    fn something_that_is_not_an_address_stops_start_up() {
        let mut config = Config::default();
        config.server.trusted_proxies = vec!["10.0.0.0/8".to_owned(), "the proxy".to_owned()];
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("trusted_proxies"), "got: {err}");
    }

    /// The spelling an operator reaches for first has to work, because the way
    /// out of a setting that will not load is the boolean that turns the
    /// throttles off.
    #[test]
    fn a_comma_separated_list_is_understood() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("TESSERA_SERVER__TRUSTED_PROXIES", "127.0.0.1, ::1");
            jail.create_file("tessera.toml", "")?;
            let config = Config::load_from(Path::new("tessera.toml")).expect("must load");
            assert_eq!(config.server.trusted_proxies, ["127.0.0.1", "::1"]);
            Ok(())
        });
    }

    #[test]
    fn a_bare_address_and_a_block_are_both_accepted() {
        let mut config = Config::default();
        config.server.trusted_proxies = vec![
            "127.0.0.1".to_owned(),
            "::1".to_owned(),
            "10.0.0.0/8".to_owned(),
        ];
        config.validate().unwrap();
    }

    /// Nothing is believed unless an operator said what to believe.
    #[test]
    fn the_default_trusts_no_proxy() {
        assert!(Config::default().server.proxy_trust().is_empty());
    }

    /// An operator part way through migrating gets the setting that names
    /// something over the one that says "anyone".
    #[test]
    fn a_list_wins_over_the_deprecated_boolean() {
        let mut config = Config::default();
        config.server.trusted_proxies = vec!["10.0.0.0/8".to_owned()];
        config.server.trusted_proxy = true;

        let trust = config.server.proxy_trust();
        let chain = HeaderMap::new();
        // 127.0.0.1 is outside the list, so it is not a proxy and gets no say —
        // which the boolean on its own would have granted it.
        assert!(
            !trust
                .resolve(Some("127.0.0.1".parse().unwrap()), &chain)
                .via_trusted_proxy
        );
    }

    #[test]
    fn the_deprecated_boolean_still_works_on_its_own() {
        let mut config = Config::default();
        config.server.trusted_proxy = true;
        let trust = config.server.proxy_trust();
        assert!(!trust.is_empty());
        assert!(
            trust
                .resolve(Some("127.0.0.1".parse().unwrap()), &HeaderMap::new())
                .via_trusted_proxy
        );
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
