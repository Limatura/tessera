//! Configuration loading and validation. See `docs/configuration.md`.
//!
//! Precedence, lowest to highest: built-in defaults → environment variables
//! (`TESSERA_` prefix, `__` separates nesting levels) → the `settings` table
//! the admin panel writes. There is no configuration file; the environment
//! holds only what the panel cannot change at runtime.
//!
//! [`Config::load`] validates everything, so bad values fail at start-up. A bad
//! value from the *table* is dropped instead ([`Config::load_with_overrides`]),
//! so the panel cannot make an instance unbootable.

use std::{collections::BTreeMap, net::IpAddr, time::Duration};

use figment::{
    Figment,
    providers::{Env, Serialized},
};
use serde::{Deserialize, Serialize};
use tessera_core::{Accent, Mode, PasswordPolicy, RetrievalCode, Theme};

use crate::auth::{cookie::SecurePolicy, throttle};

/// Prefix for every environment variable that feeds configuration.
pub const ENV_PREFIX: &str = "TESSERA_";

/// Separator used to express nesting in environment variables,
/// e.g. `TESSERA_SERVER__PORT=8080`.
pub const ENV_NESTED_SEPARATOR: &str = "__";

/// Variables that share the [`ENV_PREFIX`] namespace without being settings.
///
/// [`Config`] is `deny_unknown_fields`, so a typo like `TESSERA_SERVER__PROT`
/// stops the process instead of being silently ignored. The cost is that
/// anything else in the namespace is read as a setting too, and both of these
/// would otherwise abort start-up:
///
/// - `CONFIG_FILE` selects the file to read, so it cannot be a field *in* it.
/// - `BUILD_REV` is consumed by `build.rs` at compile time, and cargo re-exports it to the process
///   it builds. That broke every `cargo run` until this list existed.
///
/// Keep it minimal: every entry is a name a user can no longer typo-check.
const NON_CONFIG_ENV_KEYS: &[&str] = &["CONFIG_FILE", "BUILD_REV"];

/// The variable that used to name a configuration file.
///
/// There is no file layer any more. It stays in [`NON_CONFIG_ENV_KEYS`] so an
/// instance upgrading with it still set boots instead of failing
/// `deny_unknown_fields`. [`Config::load`] warns about it, because a file that
/// is silently ignored looks like a file that is being read.
const RETIRED_CONFIG_FILE_ENV: &str = "TESSERA_CONFIG_FILE";

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
    /// The picture a chat client shows for a share link.
    #[serde(default)]
    pub og: OgConfig,
    /// How the site looks to a visitor who has not picked a look of their own.
    #[serde(default)]
    pub appearance: AppearanceConfig,
    /// Where uploads are sent to be checked for malware. See [`ScanConfig`].
    #[serde(default)]
    pub scan: ScanConfig,
    /// What this instance calls itself. See [`SiteConfig`].
    #[serde(default)]
    pub site: SiteConfig,
    /// Measuring the line to this instance. See [`SpeedConfig`].
    #[serde(default)]
    pub speed: SpeedConfig,
    /// The key recoverable secrets are sealed under. See [`SecurityConfig`].
    #[serde(default)]
    pub security: SecurityConfig,
}

/// The key everything recoverable is stored under.
///
/// This used to be `storage.secret_key`, because a backend URL was the first
/// setting that had to be stored and read back. Five more followed (an OIDC
/// client secret, two OAuth ones, the mail relay's, the scanner's), none of
/// them about object storage, and the old name sent operators looking for a
/// bucket setting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SecurityConfig {
    /// Seals the settings kept in the database, as 64 hex characters.
    ///
    /// Empty (the default) uses a key generated on first start and kept in
    /// `secret.key` under `server.data_dir`. Set one (`openssl rand -hex 32`)
    /// to keep it out of that directory and its backups. See
    /// [`security::secret`](crate::security::secret) for what that does and
    /// does not protect against.
    ///
    /// Lose it and every sealed setting and backend has to be entered again.
    pub secret_key: String,
}

/// The speed test: timing a transfer of meaningless bytes.
///
/// Off by default because every test costs the instance the bytes it moves,
/// and an open endpoint costs whatever anyone asks for. Hence the layers: off,
/// then accounts only, then everyone, with every single test capped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpeedConfig {
    /// Whether the endpoints exist at all.
    pub enabled: bool,
    /// Whether visitors without an account may use them.
    ///
    /// When off, a visitor sees nothing: the capability reports the test as
    /// unavailable and the endpoints answer as if they were not mounted.
    pub anonymous: bool,
    /// The most one test may move, in each direction.
    pub max_bytes: u64,
}

impl Default for SpeedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            anonymous: false,
            // Enough for a couple of seconds on a fast line; the per-source
            // limit then bounds an hour at a few hundred megabytes.
            max_bytes: 25 * 1024 * 1024,
        }
    }
}

impl SpeedConfig {
    /// The largest test this instance will run. Zero is not a measurement.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled && self.max_bytes == 0 {
            return Err(ConfigError::Invalid {
                field: "speed.max_bytes",
                reason: "must be at least one byte".to_owned(),
            });
        }
        Ok(())
    }
}

/// What the instance calls itself.
///
/// Written into `index.html` as it is served, like the appearance, so the tab
/// shows the operator's name on first paint. See [`crate::assets`].
///
/// This is not SEO. Share pages should not be indexed, so ranking tags are left
/// out (see the settings the panel does not offer). These two cover what an
/// operator needs to brand the instance: the tab title and the line a chat
/// client shows under a link to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SiteConfig {
    /// The tab's title, and the name a link preview gives the instance.
    pub title: String,
    /// The line under the instance's own links. Empty leaves it out.
    pub description: String,
    /// The language of what the server writes for visitors: a share link's
    /// preview and its card.
    ///
    /// The instance's, not the request's. Whatever unfurls a link fetches it
    /// with no language preference of its own, and everyone in the channel
    /// sees the one preview it made.
    pub language: Language,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            title: "Tessera".to_owned(),
            description: String::new(),
            language: Language::default(),
        }
    }
}

/// A language the server can write in. The same three the web app ships.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    /// English.
    #[default]
    #[serde(rename = "en")]
    En,
    /// Traditional Chinese, as written in Taiwan.
    #[serde(rename = "zh-TW")]
    ZhTw,
    /// Japanese.
    #[serde(rename = "ja")]
    Ja,
}

impl SiteConfig {
    /// Longest description. Chat clients cut off around two hundred.
    const MAX_DESCRIPTION: usize = 300;
    /// Longest title accepted. A tab shows perhaps a fifth of this.
    const MAX_TITLE: usize = 100;

    fn validate(&self) -> Result<(), ConfigError> {
        let too_long = |field: &'static str, max: usize| ConfigError::Invalid {
            field,
            reason: format!("must be at most {max} characters"),
        };
        if self.title.trim().is_empty() {
            return Err(ConfigError::Invalid {
                field: "site.title",
                reason: "must not be empty".to_owned(),
            });
        }
        if self.title.chars().count() > Self::MAX_TITLE {
            return Err(too_long("site.title", Self::MAX_TITLE));
        }
        if self.description.chars().count() > Self::MAX_DESCRIPTION {
            return Err(too_long("site.description", Self::MAX_DESCRIPTION));
        }
        Ok(())
    }
}

/// A malware scanner this instance hands uploads to.
///
/// A separate service, not a library: ClamAV's signatures are hundreds of
/// megabytes and change daily, so bundling them would bloat the image and
/// leave it out of date. An operator who wants scanning runs a scanner and
/// points this at it.
///
/// A recorded file is read back from the store and scanned in the background,
/// since large files take minutes and files written straight to a bucket never
/// passed through the server. Its share cannot be downloaded until a verdict
/// comes back. If the scanner is unreachable nothing is held up: the file is
/// served and scanned once the scanner returns. See `docs/scanning.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ScanConfig {
    /// The scanner. Empty (the default) disables scanning.
    ///
    /// `clamd://host:3310` for a ClamAV daemon, or an `http://` / `https://`
    /// URL that takes the file as a `POST` body and answers
    /// `{"clean": true}` or `{"clean": false, "threat": "..."}`.
    pub url: String,
    /// Sent as `Authorization` on every request to an HTTP scanner, when set.
    pub authorization: String,
}

impl ScanConfig {
    /// Whether uploads are being scanned.
    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        !self.url.is_empty()
    }

    fn validate(&self) -> Result<(), ConfigError> {
        if !self.is_enabled() {
            return Ok(());
        }
        let refuse = |reason: &str| ConfigError::Invalid {
            field: "scan.url",
            reason: reason.to_owned(),
        };
        let url = url::Url::parse(&self.url)
            .map_err(|_| refuse("must be a URL such as `clamd://localhost:3310`"))?;
        match url.scheme() {
            "clamd" if url.host_str().is_some() && url.port().is_some() => Ok(()),
            "clamd" => Err(refuse("a clamd address needs a host and a port")),
            "http" | "https" => Ok(()),
            _ => Err(refuse(
                "must start with `clamd://`, `http://` or `https://`",
            )),
        }
    }
}

/// The instance's default answer to the three appearance questions.
///
/// Applies to signed-out visitors, and to signed-in users for whichever of the
/// three they have not set. Served into `index.html` so the page has the right
/// colours on first paint; see [`crate::assets`].
///
/// Three settings instead of one because the panel's key list is read off this
/// struct, and a nested object is treated as a group, not a setting.
/// `PUT /admin/settings` applies all changes in one swap, so no page load can
/// see the mode changed but not the accent. Keeping them separate also means
/// the values are validated by parsing, instead of by hand-written checks on
/// a JSON blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AppearanceConfig {
    /// Light, dark, or whichever the visitor's device asks for.
    pub mode: Mode,
    /// The neutral palette.
    pub theme: Theme,
    /// The hue that carries emphasis.
    pub accent: Accent,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        Self {
            // Following the device is the only neutral default, and a visitor
            // who set their system to dark has already chosen.
            mode: Mode::System,
            theme: Theme::Default,
            accent: Accent::Violet,
        }
    }
}

/// What one plain OAuth 2.0 provider needs.
///
/// `Default` is derived: every empty value is correct, and `Config::default`
/// fills in the two per-provider fields, the scopes and the name.
///
/// Not an [`OidcConfig`] with empty fields: OIDC is configured by issuer and
/// discovers the rest. GitHub and Discord publish no discovery document, so
/// their endpoints live in their adapters and only the client credentials are
/// configured here.
///
/// The scopes matter. GitHub will not release a verified address without
/// `user:email`, nor Discord without `email`, and without one every sign-in
/// fails with `no_address`. They default to what works, and removing a
/// required one is refused at start-up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OauthConfig {
    /// This installation's identifier at the provider.
    pub client_id: String,
    /// Its secret. Sealed before it is stored; see `settings.rs`.
    pub client_secret: String,
    /// What to ask the provider for.
    pub scopes: Vec<String>,
    /// The provider's base URL, when it is not the usual one.
    ///
    /// For GitHub Enterprise Server, which serves the same endpoints under an
    /// organisation's own host. Empty means github.com. Discord ignores this;
    /// it cannot be self-hosted.
    pub base_url: String,
    /// What to write on the button. Empty falls back to the provider's name.
    pub display_name: String,
    /// Whether an address the provider has verified may attach to an existing
    /// account with that address.
    ///
    /// Off by default, as for OIDC: an account at either provider takes a
    /// minute to create, so an instance that lets strangers register and also
    /// matches on address is giving away other people's accounts.
    pub link_by_verified_email: bool,
    /// Whether a first sign-in here creates an account. See
    /// [`ProviderRegistration`].
    ///
    /// **Neither provider has groups to restrict this**: `always` lets every
    /// account at the provider create one here.
    pub allow_registration: ProviderRegistration,
}

/// Signing in through an identity provider.
///
/// An empty `issuer` (the default) turns everything off: nothing is mounted,
/// advertised or logged. Same as [`MailConfig`]: an instance that does not use
/// this should not have to think about it.
///
/// The provider decides **who may sign in**; tessera decides **what they may
/// do**. An admitted account lands in `auth.default_role` and is moved by an
/// administrator, like one registered with a password.
///
/// Group claims are read only for [`OidcConfig::required_group`] and never
/// mapped to roles. Mapping would need a group-to-role table, a rule for
/// members of two groups, handling for renamed groups, and a way to stop the
/// next sign-in overwriting an administrator's change. For an instance with a
/// handful of accounts, that is a lot of machinery to avoid setting a menu
/// once per person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OidcConfig {
    /// The provider's issuer URL, from which everything else is discovered.
    ///
    /// Empty turns this off. Otherwise it must be `https`, and it is compared
    /// with the `iss` claim by exact string equality, as the spec requires.
    pub issuer: String,

    /// This instance, as registered with the provider.
    pub client_id: String,

    /// The secret issued with `client_id`.
    ///
    /// Listed in [`SECRETS`](crate::settings), so the admin panel can write it
    /// but never read it back.
    pub client_secret: String,

    /// What to ask the provider for.
    ///
    /// `openid` is required by the spec and added if missing. `email` is
    /// required by tessera: an account needs an address, so a sign-in without
    /// one is refused instead of making the column nullable (see the
    /// `m20260905` migration).
    pub scopes: Vec<String>,

    /// Only admit members of this group. Empty admits everyone the provider
    /// authenticates.
    ///
    /// A provider serving several applications authenticates all of its users,
    /// and being known to the provider does not mean someone should have an
    /// account here. With only Google configured, that would be every Google
    /// account.
    pub required_group: String,

    /// Which claim carries the groups. `groups` for Authentik and Authelia,
    /// `realm_access.roles` for Keycloak, `memberOf` for some LDAP bridges.
    ///
    /// Dots select nested fields.
    pub group_claim: String,

    /// What the sign-in button says. Empty uses the issuer's host.
    pub display_name: String,

    /// Whether a provider identity may attach itself to an existing account
    /// with the same address. See [`LinkByEmail`].
    pub link_by_email: LinkByEmail,
    /// Whether signing in here for the first time makes an account. See
    /// [`ProviderRegistration`].
    pub allow_registration: ProviderRegistration,
}

/// Whether a provider may create accounts, independently of
/// `auth.registration`.
///
/// `auth.registration` used to cover both password sign-up and accounts
/// created on a first provider sign-in, so an instance could not close the
/// first and keep the second. That is the usual setup with a self-run
/// provider: accounts are made there, this instance has no sign-up form, and
/// users the provider admits should not wait for an administrator here.
///
/// Three values instead of a switch so the default changes nothing: `inherit`
/// is the behaviour from before this setting existed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRegistration {
    /// As `auth.registration` says, invitation included.
    #[default]
    Inherit,
    /// Always, regardless of `auth.registration` and without an invitation.
    /// Being admitted by the provider is enough.
    Always,
    /// Never. Only existing accounts, or ones their owners have linked, can
    /// sign in this way.
    Never,
}

/// When an identity provider's address may attach a sign-in to the existing
/// account with that address.
///
/// This used to be a switch that also required `email_verified`. Many
/// providers do not send it: Synology's SSO Server lacks the claim, Entra ID
/// omits it for work accounts, and Authentik's default mapping always sends
/// `false`. With those the switch had no effect, and the first sign-in failed
/// with `address_taken` while the panel said linking was on.
///
/// So the operator states how far they trust the provider's addresses. This
/// applies to OIDC only: GitHub and Discord never hand over an unverified
/// address, so a third value would change nothing there.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkByEmail {
    /// Never. A sign-in whose address is taken is refused; the user signs in
    /// with their password and links the provider from there.
    #[default]
    Never,
    /// When the provider says `email_verified` is true.
    WhenVerified,
    /// Regardless of what the provider says.
    ///
    /// **Anyone who can get the provider to vouch for an address can take over
    /// the account with that address here.** Fine for a provider the operator
    /// runs and hands out accounts on; wrong for one with open sign-up.
    Always,
}

impl LinkByEmail {
    /// Whether an address, verified by the provider or not, may attach to an
    /// existing account.
    #[must_use]
    pub const fn allows(self, verified: bool) -> bool {
        match self {
            Self::Never => false,
            Self::WhenVerified => verified,
            Self::Always => true,
        }
    }
}

impl OauthConfig {
    /// Whether enough is set to offer this provider.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.client_id.trim().is_empty() && !self.client_secret.trim().is_empty()
    }

    /// Refuses a configuration that cannot work.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when a scope the provider needs to release a
    /// verified address has been removed.
    fn validate(&self, field: &'static str, required: &str) -> Result<(), ConfigError> {
        if !self.is_configured() {
            return Ok(());
        }
        if !self.scopes.iter().any(|scope| scope == required) {
            return Err(ConfigError::Invalid {
                field,
                reason: format!(
                    "must include the `{required}` scope, or the provider releases no address \
                     and every sign-in is refused"
                ),
            });
        }
        Ok(())
    }
}

impl Default for OidcConfig {
    fn default() -> Self {
        Self {
            issuer: String::new(),
            client_id: String::new(),
            client_secret: String::new(),
            scopes: vec![
                "openid".to_owned(),
                "email".to_owned(),
                "profile".to_owned(),
            ],
            required_group: String::new(),
            group_claim: "groups".to_owned(),
            display_name: String::new(),
            link_by_email: LinkByEmail::Never,
            allow_registration: ProviderRegistration::Inherit,
        }
    }
}

impl OidcConfig {
    /// Whether this instance offers sign-in through a provider.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        !self.issuer.trim().is_empty()
    }

    /// Rejects a configuration this binary cannot honour.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] naming the field and what is wrong with it.
    fn validate(&self) -> Result<(), ConfigError> {
        if !self.is_configured() {
            return Ok(());
        }
        // Over `http` the authorisation code and client secret travel in the
        // clear, and the spec only lets us skip verifying the token signature
        // because the channel is TLS. Loopback is exempt: those bytes never
        // reach a network. See [`is_private_or_tls`].
        if !crate::auth::oidc::is_private_or_tls(&self.issuer) {
            return Err(ConfigError::Invalid {
                field: "auth.oidc.issuer",
                reason: "must be an https URL, or http on a loopback address".to_owned(),
            });
        }
        for (field, value) in [
            ("auth.oidc.client_id", &self.client_id),
            ("auth.oidc.client_secret", &self.client_secret),
        ] {
            if value.trim().is_empty() {
                return Err(ConfigError::Invalid {
                    field,
                    reason: "is required when an issuer is set".to_owned(),
                });
            }
        }
        if self.group_claim.trim().is_empty() {
            return Err(ConfigError::Invalid {
                field: "auth.oidc.group_claim",
                reason: "cannot be empty; the default is `groups`".to_owned(),
            });
        }
        Ok(())
    }

    /// The scopes to ask for, with the two this cannot work without.
    #[must_use]
    pub fn effective_scopes(&self) -> Vec<String> {
        let mut scopes = self.scopes.clone();
        for required in ["openid", "email"] {
            if !scopes.iter().any(|scope| scope == required) {
                scopes.push(required.to_owned());
            }
        }
        scopes
    }
}

/// The link-preview card, as the operator wants it.
///
/// One design for the whole instance, not one per share. See
/// [`og::layout`](crate::og::layout) and `docs/link-previews.md`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct OgConfig {
    /// The operator's own card, as HTML.
    ///
    /// Empty, the default, draws the built-in card. A layout set here replaces
    /// it for every share; what each share may reveal is still decided
    /// elsewhere, so `{{size}}` in a locked share's card comes out empty
    /// because the value does, not because the layout was careful.
    ///
    /// Rejected if it uses anything but `div`, `span` and `style`, or names a
    /// resource outside this server. See `og::layout`.
    pub layout: String,
}

/// Which parts of the service this instance exposes.
///
/// These are *runtime* switches. A disabled feature's routes are never mounted,
/// so calling one gives a plain 404 instead of a 403 that would confirm it
/// exists. Turning something off does not shrink the binary; cargo features
/// (`postgres`, `storage-s3`) do that.
///
/// `GET /api/v1/capabilities` reports the effective answer. A single-page app
/// cannot tell from a static contract which of these an instance turned on.
// The flags are independent by design, not one value in disguise: every
// combination of `accounts` and `anonymous_upload` is a real deployment.
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent switches, not a state machine; see the comment above"
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConfig {
    /// Registration, login and sessions.
    ///
    /// Off means the instance has no accounts at all, which suits one person
    /// behind a VPN who wants a file drop and nothing else.
    pub accounts: bool,

    /// Administrative endpoints, used by the separate admin front-end.
    pub admin: bool,

    /// Whether a caller without a session may upload.
    ///
    /// Independent of [`accounts`](Self::accounts) by design. Wanting an admin
    /// panel is not the same as wanting to force everyone to register.
    pub anonymous_upload: bool,

    /// The browsable API reference at `/docs`.
    ///
    /// Off by default. It is a development convenience, not part of the
    /// product.
    ///
    /// The page is a few lines of HTML that pull the renderer from a CDN.
    /// **The server never fetches it**, the browser does. A server behind a
    /// firewall still serves working docs to an operator on an ordinary
    /// connection; only an air-gapped reader gets a blank page.
    ///
    /// Keeping the renderer remote holds ~11 MB out of the binary: 23.8 MB
    /// with it embedded, 12.5 MB without.
    ///
    /// `openapi.json` is served either way. This only controls the UI.
    pub docs: bool,

    /// Whether uploads may be end-to-end encrypted.
    ///
    /// Off by default. It protects visitors from the operator, which matters on
    /// an instance run for strangers and not on a personal one, and it costs
    /// features: an encrypted share has no preview, no archive download, and
    /// nothing a server-side reader can use. See `docs/encryption.md`.
    ///
    /// Turning it off stops new encrypted shares. Existing ones stay
    /// downloadable, since the server never had to understand them.
    pub encryption: bool,
}

/// Checking a new password against known leaked passwords.
///
/// Only the first five characters of the password's SHA-1 are sent. The
/// service returns every known hash with that prefix and the comparison
/// happens here (the k-anonymity model the Pwned Passwords API is built on),
/// so neither the password nor enough of its hash to identify it leaves the
/// process.
///
/// Off by default, since it puts a third-party request on a path that has to
/// work. When on and the service is unreachable, the password is accepted: an
/// outage must not block registering or resetting a lost password.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct PwnedConfig {
    /// Whether new passwords are looked up at all.
    pub enabled: bool,
    /// Where the range API lives, without a trailing slash.
    ///
    /// The official service by default. Point this at a self-hosted
    /// implementation to avoid depending on it, or when the instance has no
    /// internet access.
    pub url: String,
}

impl Default for PwnedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            url: "https://api.pwnedpasswords.com".to_owned(),
        }
    }
}

impl PwnedConfig {
    fn validate(&self) -> Result<(), ConfigError> {
        if !self.enabled {
            return Ok(());
        }
        match url::Url::parse(&self.url) {
            Ok(url) if matches!(url.scheme(), "http" | "https") => Ok(()),
            _ => Err(ConfigError::Invalid {
                field: "auth.pwned.url",
                reason: "must be an `http://` or `https://` URL".to_owned(),
            }),
        }
    }
}

/// Who may create an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationPolicy {
    /// Anyone may register.
    Open,
    /// Only with an invitation.
    Invite,
    /// No one. An administrator creates accounts.
    Closed,
}

/// Where outgoing mail goes, when there is any.
///
/// # Empty means off, not broken
///
/// An empty `host` is the default and the common case. A single-user instance
/// gets back into a forgotten account with `reset-password` on the box itself.
/// Nothing is mounted, nothing is advertised, and start-up says nothing.
///
/// # Why not a mail server
///
/// Home connections cannot deliver mail: port 25 is blocked by most providers
/// and receiving servers reject residential addresses regardless of SPF and
/// DKIM. This expects a relay (Resend, Brevo, SES, Fastmail, or a company's
/// own): four values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailConfig {
    /// SMTP host. Empty turns off everything that sends mail.
    pub host: String,

    /// SMTP port. Relays generally want 587, submission with STARTTLS; 465 is
    /// implicit TLS and also common.
    pub port: u16,

    /// How the connection to the relay is encrypted. See [`MailTls`].
    pub tls: MailTls,

    /// SMTP username. Resend wants the literal `resend` here.
    pub username: String,

    /// SMTP password. For most relays this is an API key.
    pub password: String,

    /// The `From:` address. Must be a domain verified with the relay.
    pub from: String,

    /// Where this instance is reachable, used to build the links in mail.
    ///
    /// Configured, never taken from the request's `Host` header. A reset link
    /// built from an attacker-controlled header would point at their server and
    /// hand them the token.
    pub public_url: String,

    /// How long a password reset link works for, in minutes.
    ///
    /// Short because the link *is* the credential: long enough to survive a
    /// mail queue, capped at [`MAX_RESET_TTL_MINUTES`].
    pub reset_ttl_minutes: u64,

    /// How long a registration confirmation link works for, in hours.
    ///
    /// Much longer than a reset link, because it grants less. Following it
    /// creates an account with a password the person who asked already chose,
    /// so a stolen link gains an account its thief cannot sign into.
    pub registration_ttl_hours: u64,
}

/// How the connection to the mail relay is encrypted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailTls {
    /// Whatever the port means: implicit TLS on 465, STARTTLS on anything
    /// else.
    Auto,
    /// Opens in the clear and upgrades before anything is sent. Refuses a relay
    /// that cannot upgrade.
    Starttls,
    /// TLS from the first byte.
    Implicit,
    /// No encryption at all, for a relay on this machine or this network: a
    /// local postfix on port 25, a relay container, Mailpit.
    ///
    /// Only to private and loopback addresses. The password and every reset
    /// link would otherwise cross the internet readable, so a host that
    /// resolves anywhere else is refused when mail is sent.
    None,
}

impl MailTls {
    /// What `Auto` means on `port`; the others as they are.
    #[must_use]
    pub const fn on(self, port: u16) -> Self {
        match self {
            Self::Auto if port == 465 => Self::Implicit,
            Self::Auto => Self::Starttls,
            other => other,
        }
    }
}

/// The longest a reset link may be configured to last, in minutes.
///
/// A day. Past that it has stopped being a way back in and become a second
/// credential sitting somewhere less protected than a password.
pub const MAX_RESET_TTL_MINUTES: u64 = 24 * 60;

/// The longest a registration link may be configured to last, in hours.
///
/// A week. The row behind it holds a password hash for an account that does not
/// exist, and a registration is never confirmed a fortnight later.
pub const MAX_REGISTRATION_TTL_HOURS: u64 = 7 * 24;

impl Default for MailConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 587,
            tls: MailTls::Auto,
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
    /// Saturating multiplication. Validation caps the value, but this is `pub`,
    /// and a caller who built a `MailConfig` by hand should get a long duration
    /// instead of a debug panic and a release wraparound.
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

    /// Signing in through an identity provider.
    #[serde(default)]
    pub oidc: OidcConfig,

    /// Signing in with a GitHub account.
    #[serde(default)]
    pub github: OauthConfig,

    /// Signing in with a Discord account.
    #[serde(default)]
    pub discord: OauthConfig,

    /// How long a browser session stays valid, in hours.
    pub session_ttl_hours: u64,
    /// How long an ended session stays in the list of logins, in days.
    ///
    /// The rows answer "has anybody else been in here", so they outlive the
    /// credential. Long enough to cover last month, short enough that a home
    /// instance is not carrying a year of them.
    #[serde(default = "default_login_history_days")]
    pub login_history_days: u64,

    /// What an account password must be: its length, and the character
    /// classes it needs, all off by default. See [`PasswordPolicy`].
    pub password: PasswordPolicy,

    /// Checking new passwords against known breaches. See [`PwnedConfig`].
    #[serde(default)]
    pub pwned: PwnedConfig,

    /// What one password hash costs. See [`HashingConfig`].
    #[serde(default)]
    pub hashing: HashingConfig,

    /// How hard the instance is on repeated attempts. See [`ThrottleConfig`].
    #[serde(default)]
    pub throttle: ThrottleConfig,

    /// When to mark the session cookie `Secure`.
    pub cookie_secure: SecurePolicy,

    /// Which role a newly registered account goes into.
    ///
    /// By name, because that is what an operator reads in the admin panel.
    ///
    /// # Why not just edit the built-in role
    ///
    /// If ordinary users should do less, point this at a role of your own.
    /// Redefining `member` makes it mean something different on every
    /// instance, and then no documentation can describe it.
    ///
    /// Startup checks that the role exists, so a missing one fails there
    /// instead of turning a stranger's registration into a `500`.
    #[serde(default = "default_role")]
    pub default_role: String,

    /// Name for the administrator created on first run.
    pub admin_username: String,

    /// Address for the administrator created on first run.
    pub admin_email: String,

    /// Password for the administrator created on first run.
    ///
    /// Empty means "generate one and print it once". That is also the safer
    /// default, because a password written into a compose file tends to stay
    /// there unchanged.
    pub admin_password: String,
}

/// How shares are created and handed out.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShareConfig {
    /// Whether retrievals are written down, and for how long. See
    /// [`AccessLogConfig`].
    #[serde(default)]
    pub access_log: AccessLogConfig,

    /// Characters in a generated retrieval code.
    ///
    /// A short code is quicker to read out loud, a long one is not worth
    /// scanning for. Eight Crockford base32 characters is about 1.1 x 10^12
    /// combinations; five, as some comparable services use, is 33 million,
    /// which a public endpoint can be walked through. Short codes are only
    /// defensible behind a throttle on failed lookups.
    pub code_length: usize,

    /// Content at or below this size stays in the database instead of the blob
    /// store.
    ///
    /// Most shares on a personal instance are a short note or a snippet, where
    /// a round trip to object storage costs more than it saves. A database
    /// backup then covers the content too. Larger content goes to the store,
    /// where it can be streamed instead of loaded.
    pub inline_threshold_bytes: usize,
    /// The largest a text share may be, in bytes.
    ///
    /// Text lives in the database, so this is the largest row this service will
    /// write and the most memory one upload can hold at once. A value handed
    /// over whole cannot be streamed.
    ///
    /// A megabyte is about a million characters. Anything larger is not a
    /// paste and belongs in a file share, which does stream.
    pub max_text_bytes: usize,

    /// How long a download authorisation stays usable, in hours.
    ///
    /// One authorisation spends one of a share's permitted retrievals; the
    /// resulting ticket may then be presented as often as the transfer needs
    /// (`Range` requests, a resumed download, a retry after a dropped
    /// connection). Without that separation a single interrupted download would
    /// burn a one-time share.
    ///
    /// This is the ceiling, not the usual life; most tickets die of
    /// [`ticket_idle_hours`] first. It is long because ten gigabytes over a
    /// slow link takes most of a day, and the holder already has the right to
    /// the content.
    ///
    /// [`ticket_idle_hours`]: Self::ticket_idle_hours
    pub ticket_ttl_hours: u64,

    /// How long a download authorisation survives without being used, in hours.
    ///
    /// # Why idleness and not a fixed life
    ///
    /// A ticket is a string, so it can be forwarded. A fixed life has to cover
    /// the slowest link finishing the largest file, and a ticket that lives
    /// that long is a ticket worth forwarding.
    ///
    /// Every presentation pushes the expiry out. A live transfer keeps its
    /// authorisation; a finished one dies within the hour. This asks whether
    /// the download is still alive, the same question
    /// `server.upload_stall_secs` asks on the way in. Scaling by file size
    /// cannot work: ten gigabytes is eighty seconds on gigabit and seventy
    /// minutes on twenty megabits, and the server cannot see the line.
    pub ticket_idle_hours: u64,

    /// Lifetime a share gets when the request does not ask for one, in hours.
    ///
    /// This decides how much disk an instance holds, more than any quota does.
    /// What is on disk is roughly the upload rate times how long things are
    /// kept, so an instance keeping shares for a week holds a week of uploads
    /// however long it runs.
    ///
    /// A day is too short when the recipient is asleep or in another timezone.
    /// A month is too long when most shares are collected within hours.
    ///
    /// `0` means "use [`max_expiry_hours`](Self::max_expiry_hours)".
    pub default_expiry_hours: u64,

    /// Longest lifetime a share may be given, in hours. `0` means no ceiling.
    pub max_expiry_hours: u64,

    /// How long a finished share is kept before it is removed, in hours.
    ///
    /// A share that expired or was used up is kept a while, so following its
    /// link says "this expired" instead of "no such code". Only one of those
    /// answers is worth retyping. After this, the row and its blob both go.
    ///
    /// `0` removes them on the next sweep.
    pub retain_after_death_hours: u64,

    /// How long an unfinished upload waits before the sweep removes it.
    ///
    /// It holds real bytes that count against the quota the whole time, so this
    /// is how long an upload can reserve space after being abandoned. Long
    /// enough that a laptop closed overnight can still resume, short enough
    /// that dead sessions do not pile up.
    ///
    /// # On S3, set a lifecycle rule as well
    ///
    /// The sweep aborts what it finds, but a failed abort leaves parts at the
    /// store and S3 never removes an abandoned multipart upload by itself. Set
    /// an `AbortIncompleteMultipartUpload` rule a few days longer than this
    /// value so storage cannot leak while this instance is offline.
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
    /// Does not apply to upload routes. See `upload_stall_secs`.
    pub request_timeout_secs: u64,

    /// How long an upload may go without a byte arriving, in seconds.
    ///
    /// # Why uploads are not given a wall-clock budget
    ///
    /// One number cannot be both. Letting a 5 GB upload finish over a 20 Mbit/s
    /// line needs 2400 seconds, and that is also how long a client that
    /// connects and sends nothing would hold a slot. The two demands are three
    /// orders of magnitude apart.
    ///
    /// So this asks a different question: is the other end still sending? A
    /// 5 GB upload takes as long as it takes; a dead connection is dropped
    /// after this many seconds.
    pub upload_stall_secs: u64,
    /// Maximum accepted request body, in bytes.
    pub max_body_bytes: usize,

    /// How much of an upload is gathered before a part goes to the store.
    ///
    /// Only reaches an object store that does multipart. S3 refuses every part
    /// but the last below 5 MiB. Cloudflare R2 also requires every part but the
    /// last to be the *same* size, so on R2 this should equal the minimum, as
    /// tusd does. Garage enforces neither, so the CI job cannot catch a mistake
    /// here.
    pub part_size_bytes: usize,

    /// Where this instance is reachable from outside, as an absolute URL.
    ///
    /// Only link previews need it, and they need it absolute. `og:image` is
    /// fetched by a crawler on another machine.
    ///
    /// Empty, the default, derives it from each request's `Host` and forwarded
    /// scheme. That breaks in the setups this is aimed at: a reverse proxy that
    /// rewrites `Host`, or a container that only ever sees `localhost:8080`. If
    /// previews come out blank, set this.
    ///
    /// A trailing slash is trimmed, so both spellings behave the same.
    #[serde(default)]
    pub public_url: String,

    /// Directory to serve the frontend from, instead of the bundle compiled in.
    ///
    /// Empty, the default, serves the compiled-in bundle. One file, and no web
    /// server in front of it.
    ///
    /// Set, every request reads from disk, so rebuilding the frontend and
    /// refreshing the browser is the whole loop. Without it, changing one line
    /// of CSS in a container means waiting for a Rust build.
    ///
    /// A directory with no `index.html` stops the server. Falling back to the
    /// compiled-in bundle would quietly serve yesterday's build.
    #[serde(default)]
    pub assets_dir: String,

    /// Where this instance keeps files it writes for itself.
    ///
    /// Two, both only on first start: `secret.key`, the key for sealed
    /// settings, when `security.secret_key` is not set; and a `blobs`
    /// directory as the first storage backend, when there is none. Relative
    /// paths are resolved from the working directory.
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
    /// Origins allowed to call the API from a browser. Empty means same-origin
    /// only, the right default for the single-binary deployment.
    pub cors_allowed_origins: Vec<String>,

    /// Which proxies in front of this service may be believed, as CIDR blocks
    /// or bare addresses.
    ///
    /// Empty by default, meaning nothing is believed: `X-Forwarded-For` and
    /// `X-Forwarded-Proto` are set by whoever spoke to us last, which on a
    /// direct connection is the caller. Believing them from anyone would let a
    /// caller pick their own source address, which every throttle here counts
    /// against, and claim HTTPS on a plain connection.
    ///
    /// `["127.0.0.1", "::1"]` covers a proxy on the same host;
    /// `["10.0.0.0/8"]` a container network.
    #[serde(default, deserialize_with = "list_or_commas")]
    pub trusted_proxies: Vec<String>,

    /// Deprecated: use [`ServerConfig::trusted_proxies`].
    ///
    /// `true` means "whatever connects to us is a proxy", so anyone who can
    /// reach the port directly can forge the headers. It still works, so an
    /// upgrade does not lock an operator out of their own instance, and
    /// start-up warns about it.
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
    /// Without this, an instance with `anonymous_upload` on (the default) is an
    /// unlimited anonymous drive for anyone who can reach it, and the first
    /// sign of trouble is writes failing on a full disk.
    ///
    /// Counts content that still exists, wherever it lives: content under
    /// `share.inline_threshold_bytes` sits in the database and is still disk. A
    /// share kept past its death so its link can say "this expired" has already
    /// had its bytes removed from both places and does not count.
    ///
    /// Approximate under concurrency. Uploads in flight together can land past
    /// the line by up to their own number times `server.max_body_bytes`, and
    /// the next one is refused. Leave the same slack you would on a disk.
    ///
    /// `share.default_expiry_hours` is usually the more effective lever:
    /// shortening it lowers the steady state without refusing anybody.
    pub max_total_bytes: u64,

    /// How often to reconcile the store against the database, in hours. `0`
    /// turns it off.
    ///
    /// Separate from the hourly sweep, because it costs a full listing of the
    /// store where the sweep costs a `DELETE` over an indexed column. Daily
    /// bounds a leak to a day without paying for a listing every hour.
    ///
    /// Most object stores bill a listing as API calls, metered separately from
    /// storage. Lengthen this if your transaction budget is tighter than your
    /// byte budget; shorten it on a local disk.
    pub reconcile_interval_hours: u64,

    /// How old an unreferenced object must be before the reconciliation
    /// removes it, in hours.
    ///
    /// An object is written to the store before the row naming it, so for a
    /// moment every upload looks like an orphan. This is the margin around that
    /// moment, and it is generous: waiting an extra day to reclaim leaked bytes
    /// costs nothing, while deleting a file the instant before its row lands
    /// costs the file.
    pub reconcile_grace_hours: u64,

    /// Whether one file may be spread across several backends.
    ///
    /// Off by default. It buys a file larger than any single account will hold:
    /// five free 10 GB buckets take a 40 GB file with this on and refuse it
    /// with `507` without.
    ///
    /// The cost is that a split file has no direct link, since a presigned URL
    /// names one object, so its bytes come back through this server or are
    /// stitched in the browser. On free tiers that is upload bandwidth *and*
    /// download bandwidth.
    ///
    /// A file is only split when nothing can take it whole. See
    /// `docs/splitting.md`.
    pub split_across_backends: bool,
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

/// `data`, beside the default database, so a fresh checkout keeps everything
/// it writes in one place.
fn default_data_dir() -> String {
    "data".to_owned()
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
            public_url: String::new(),
            assets_dir: String::new(),
            data_dir: default_data_dir(),
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
            // No limit: the usual deployment is one person on their own
            // hardware. `Config::load` warns if anonymous uploads are also on.
            max_total_bytes: 0,
            split_across_backends: false,
            reconcile_interval_hours: 24,
            reconcile_grace_hours: 24,
        }
    }
}

impl StorageConfig {
    /// How often to reconcile, or `None` when it is switched off.
    #[must_use]
    pub const fn reconcile_interval(&self) -> Option<Duration> {
        if self.reconcile_interval_hours == 0 {
            return None;
        }
        Some(Duration::from_secs(
            self.reconcile_interval_hours.saturating_mul(3600),
        ))
    }

    /// How old an unreferenced object must be before it is removed.
    #[must_use]
    pub const fn reconcile_grace(&self) -> Duration {
        Duration::from_secs(self.reconcile_grace_hours.saturating_mul(3600))
    }
}

impl Default for FeatureConfig {
    fn default() -> Self {
        // Everything on: the shipped image should work out of the box, and an
        // operator who wants less says so.
        Self {
            accounts: true,
            admin: true,
            anonymous_upload: true,
            docs: false,
            encryption: false,
        }
    }
}

/// Argon2id cost.
///
/// # Why this is a setting
///
/// The defaults are OWASP's current recommendation, right for a machine with
/// memory to spare. 19 MiB per hash in flight is real on a four-gigabyte Pi
/// serving a burst of sign-ins, and bigger hardware can afford more.
///
/// # Changing these does not invalidate anybody's password
///
/// The parameters are written into the hash itself and verification reads them
/// from there, so only new hashes use the new cost. Re-hashing old ones on the
/// next successful sign-in is a separate decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HashingConfig {
    /// Memory each hash holds, in kibibytes.
    ///
    /// The parameter that matters most: memory is what makes cracking on a GPU
    /// expensive. It is also **the memory cost of a sign-in burst**, multiplied
    /// by how many hashes run at once.
    pub memory_kib: u32,
    /// How many passes over that memory.
    pub iterations: u32,
    /// How many lanes run in parallel.
    pub lanes: u32,
}

impl Default for HashingConfig {
    fn default() -> Self {
        // OWASP's current recommendation for Argon2id.
        Self {
            memory_kib: 19 * 1024,
            iterations: 2,
            lanes: 1,
        }
    }
}

impl HashingConfig {
    /// Rejects a cost this binary cannot honour.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] for a value Argon2 refuses, or one so
    /// low the resulting hash would be cheap to crack.
    fn validate(&self) -> Result<(), ConfigError> {
        // Argon2's own floor. Below it `Params::new` fails at the first
        // sign-in instead of at start-up.
        if self.memory_kib < 8 * self.lanes {
            return Err(ConfigError::Invalid {
                field: "auth.hashing.memory_kib",
                reason: "must be at least eight times `lanes`, which is Argon2's own floor"
                    .to_owned(),
            });
        }
        // OWASP's minimum for Argon2id with one lane. Below it the hash can be
        // computed in bulk, which defeats the point of hashing.
        if self.memory_kib < 7 * 1024 {
            return Err(ConfigError::Invalid {
                field: "auth.hashing.memory_kib",
                reason: "must be at least 7168 (7 MiB); below that the hash is cheap to \
                         compute in bulk, which is what it exists to prevent"
                    .to_owned(),
            });
        }
        if self.iterations == 0 || self.lanes == 0 {
            return Err(ConfigError::Invalid {
                field: "auth.hashing",
                reason: "`iterations` and `lanes` must be at least 1".to_owned(),
            });
        }
        Ok(())
    }
}

/// One counter's limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    /// Attempts before the key is refused.
    pub max_attempts: u32,
    /// How long the window lasts, and how long a refusal holds, in seconds.
    pub window_secs: u64,
}

impl Limit {
    const fn new(max_attempts: u32, window: Duration) -> Self {
        Self {
            max_attempts,
            window_secs: window.as_secs(),
        }
    }

    /// The window as a duration.
    #[must_use]
    pub const fn window(&self) -> Duration {
        Duration::from_secs(self.window_secs)
    }

    fn validate(&self, field: &'static str) -> Result<(), ConfigError> {
        // Zero attempts locks out every caller; on the source counter that is
        // the whole instance. A zero window never refuses at all, so it reads
        // as on and behaves as off.
        if self.max_attempts == 0 || self.window_secs == 0 {
            return Err(ConfigError::Invalid {
                field,
                reason: "`max_attempts` and `window_secs` must both be at least 1; \
                         to run without a limit, raise `max_attempts` rather than \
                         setting either to zero"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

/// How hard the instance is on repeated attempts.
///
/// Six counters, because they protect different things. See [`Throttles`] and
/// `docs/auth.md`.
///
/// [`Throttles`]: crate::auth::throttle::Throttles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThrottleConfig {
    /// How many keys to track before expired ones are swept.
    ///
    /// A memory ceiling, not a security setting: each key is a string chosen by
    /// an unauthenticated client, so this caps how much memory they can make
    /// the process hold. Shared by every counter.
    #[serde(default = "default_tracked_keys")]
    pub max_tracked_keys: usize,

    /// Failed sign-ins, keyed on the account and the source.
    #[serde(default = "default_login_limit")]
    pub login: Limit,
    /// Anything expensive, keyed on the source alone. Loose by design:
    /// addresses are shared, so refusing one refuses everyone behind it.
    #[serde(default = "default_source_limit")]
    pub source: Limit,
    /// Retrieval codes tried and not found, keyed on the source.
    #[serde(default = "default_lookup_limit")]
    pub lookup: Limit,
    /// Password reset mails, keyed on the address asked about.
    #[serde(default = "default_mail_limit")]
    pub reset: Limit,
    /// Registration mails, keyed on the address asked about.
    #[serde(default = "default_mail_limit")]
    pub registration: Limit,
    /// Address verification mails, keyed on the address they go to.
    #[serde(default = "default_mail_limit")]
    pub verification: Limit,
    /// Failed share passwords, keyed on the code and the source.
    #[serde(default = "default_login_limit")]
    pub share: Limit,
    /// Speed tests, keyed on the source. Every test counts, not just failures:
    /// the limit is how much bandwidth one address may use.
    #[serde(default = "default_speed_limit")]
    pub speed: Limit,
    /// Breach-list lookups while a form is being typed into, keyed on the
    /// source. Every lookup counts, since each is a third-party request made
    /// on an anonymous client's behalf.
    #[serde(default = "default_pwned_limit")]
    pub pwned: Limit,
}

/// The most keys an instance may be configured to hold.
///
/// Each entry is a caller-chosen string plus a counter, a few hundred bytes
/// with the map's overhead, so the ceiling is tens of megabytes. The bound
/// exists because an attacker is the one filling this map.
const MAX_TRACKED_KEYS_CEILING: usize = 100_000;

const fn default_tracked_keys() -> usize {
    throttle::MAX_TRACKED_KEYS
}

const fn default_login_limit() -> Limit {
    Limit::new(throttle::MAX_ATTEMPTS, throttle::WINDOW)
}

const fn default_source_limit() -> Limit {
    Limit::new(throttle::SOURCE_MAX_ATTEMPTS, throttle::SOURCE_WINDOW)
}

const fn default_lookup_limit() -> Limit {
    Limit::new(throttle::LOOKUP_MAX_ATTEMPTS, throttle::SOURCE_WINDOW)
}

const fn default_pwned_limit() -> Limit {
    Limit::new(throttle::PWNED_MAX_LOOKUPS, throttle::SOURCE_WINDOW)
}

const fn default_speed_limit() -> Limit {
    Limit::new(throttle::SPEED_MAX_TESTS, throttle::RESET_WINDOW)
}

const fn default_mail_limit() -> Limit {
    Limit::new(throttle::RESET_MAX_REQUESTS, throttle::RESET_WINDOW)
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            max_tracked_keys: default_tracked_keys(),
            login: default_login_limit(),
            source: default_source_limit(),
            lookup: default_lookup_limit(),
            reset: default_mail_limit(),
            registration: default_mail_limit(),
            verification: default_mail_limit(),
            share: default_login_limit(),
            speed: default_speed_limit(),
            pwned: default_pwned_limit(),
        }
    }
}

impl ThrottleConfig {
    /// Rejects limits that would not protect anything, or that would let a
    /// stranger exhaust memory.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] for a zeroed limit or a key bound
    /// above [`MAX_TRACKED_KEYS_CEILING`].
    fn validate(&self) -> Result<(), ConfigError> {
        if self.max_tracked_keys == 0 || self.max_tracked_keys > MAX_TRACKED_KEYS_CEILING {
            return Err(ConfigError::Invalid {
                field: "auth.throttle.max_tracked_keys",
                reason: format!(
                    "must be between 1 and {MAX_TRACKED_KEYS_CEILING}; it is the memory an \
                     unauthenticated stranger may make this process hold"
                ),
            });
        }
        self.login.validate("auth.throttle.login")?;
        self.source.validate("auth.throttle.source")?;
        self.lookup.validate("auth.throttle.lookup")?;
        self.reset.validate("auth.throttle.reset")?;
        self.registration.validate("auth.throttle.registration")?;
        self.verification.validate("auth.throttle.verification")?;
        self.share.validate("auth.throttle.share")?;
        Ok(())
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            oidc: OidcConfig::default(),
            // The scopes each provider needs to release a *verified* address.
            // GitHub answers `email: null` on `/user` for a private address,
            // which then has to come from `/user/emails` and needs `user:email`.
            // Discord returns `email` and `verified` on `/users/@me` behind
            // `email`.
            github: OauthConfig {
                scopes: vec!["read:user".to_owned(), "user:email".to_owned()],
                display_name: "GitHub".to_owned(),
                ..OauthConfig::default()
            },
            discord: OauthConfig {
                scopes: vec!["identify".to_owned(), "email".to_owned()],
                display_name: "Discord".to_owned(),
                ..OauthConfig::default()
            },
            // Open by default, for an instance set up among people who know
            // each other. An internet-facing one should close it.
            registration: RegistrationPolicy::Open,
            default_role: default_role(),
            // 30 days. This is an occasional-use tool, and being logged out
            // between uses is friction.
            session_ttl_hours: 24 * 30,
            login_history_days: default_login_history_days(),
            hashing: HashingConfig::default(),
            throttle: ThrottleConfig::default(),
            password: PasswordPolicy::default(),
            pwned: PwnedConfig::default(),
            cookie_secure: SecurePolicy::default(),
            admin_username: "admin".to_owned(),
            admin_email: "admin@localhost.invalid".to_owned(),
            admin_password: String::new(),
        }
    }
}

/// Whether, and for how long, retrievals are written down.
///
/// # Why this is off by default
///
/// The counters need no switch. `view_count` is an integer and says nothing
/// about where a visitor was. A row per retrieval is an address and a browser
/// string, recorded about a person who was handed a link and agreed to nothing.
/// That is the operator's decision to make, not a default to inherit, and an
/// instance that turns it on should say so on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessLogConfig {
    /// Whether to write a row for each retrieval.
    pub enabled: bool,
    /// How many days a row is kept before the sweep removes it.
    ///
    /// This is the only promise the feature makes about how long a visit is
    /// remembered. Zero is refused; reading it as "forever" would be the wrong
    /// default for a setting about other people's data.
    pub retention_days: u64,
}

impl Default for AccessLogConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            // Long enough to answer "who fetched this last month", short
            // enough not to accumulate a year of visitor records.
            retention_days: 30,
        }
    }
}

impl AccessLogConfig {
    /// How long a row is kept.
    #[must_use]
    pub const fn retention(&self) -> Duration {
        Duration::from_secs(self.retention_days.saturating_mul(24 * 3600))
    }

    /// Rejects a retention this binary will not honour.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] when the log is on with no retention.
    fn validate(&self) -> Result<(), ConfigError> {
        if self.enabled && self.retention_days == 0 {
            return Err(ConfigError::Invalid {
                field: "share.access_log.retention_days",
                reason: "must be at least 1 while the access log is on; there is no value that \
                         means \"keep for ever\""
                    .to_owned(),
            });
        }
        Ok(())
    }
}

impl Default for ShareConfig {
    fn default() -> Self {
        Self {
            access_log: AccessLogConfig::default(),
            code_length: 8,
            inline_threshold_bytes: 64 * 1024,
            max_text_bytes: 1024 * 1024,
            ticket_ttl_hours: 24,
            // An hour of silence means the transfer is over. It survives a
            // paused download or a client retrying with backoff, and a
            // forwarded ticket has usually expired by the time it is used.
            ticket_idle_hours: 1,
            // A week. Almost no caller asks for a lifetime, so this and not the
            // ceiling below decides how much disk an instance holds.
            default_expiry_hours: 24 * 7,
            // 30 days, far above the default. This is the longest anyone may
            // *ask* for, not what they get by default.
            max_expiry_hours: 24 * 30,
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
/// An environment variable carries its structure inside a string, and the JSON
/// figment expects for `TESSERA_SERVER__TRUSTED_PROXIES` (`["127.0.0.1","::1"]`)
/// is not what anybody writes first. This setting decides whether the throttles
/// work, and the way out of one that will not load is the deprecated boolean
/// that turns them off, so the obvious spelling has to be accepted.
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
    /// way through migrating gets the specific setting instead of "anyone".
    ///
    /// [`Config::validate`] has already rejected an unusable entry by the time
    /// this runs, so the fallback to trusting nothing is unreachable.
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
/// The sweep is what stops the table growing without limit, and a value in the
/// thousands turns it off without saying so.
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
    /// Loads and validates configuration from the defaults and the environment.
    ///
    /// Without the stored overrides. This is what the process starts from
    /// before it has a database to read them out of. See
    /// [`Config::load_with_overrides`] for the full stack.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] if a source cannot be parsed, if a value is
    /// unusable, or if the configuration selects a backend this binary was not
    /// compiled with.
    pub fn load() -> Result<Self, ConfigError> {
        if std::env::var_os(RETIRED_CONFIG_FILE_ENV).is_some() {
            tracing::warn!(
                variable = RETIRED_CONFIG_FILE_ENV,
                "this no longer does anything: settings come from the environment and the \
                 admin panel. Anything that file sets is being ignored."
            );
        }
        Self::load_with_overrides(&BTreeMap::new()).map(|(config, _)| config)
    }

    /// What this binary ships with, before anything is layered on it.
    ///
    /// Split out because [`Config::load_with_overrides`] rebuilds the stack
    /// once per rejected setting and has to build the same one each time.
    pub(crate) fn defaults() -> Figment {
        Figment::from(Serialized::defaults(Self::default()))
    }

    /// Loads with the stored overrides layered on top.
    ///
    /// # Why the overrides win
    ///
    /// They are the only layer edited by hand *on this instance*. The
    /// environment arrives with the deployment, and the image sets six
    /// variables of its own, so letting it beat the panel would turn the panel
    /// into a screen that reports success and changes nothing.
    ///
    /// # Why a bad stored value cannot stop the process
    ///
    /// The panel can write a value this binary refuses: out of range, wrong
    /// type, or a key that was fine in the version that wrote it. If that
    /// stopped start-up, the panel could make the instance unbootable, and the
    /// instance has to be running to serve the panel that would undo it. So an
    /// override that will not load is dropped and logged, one key at a time,
    /// leaving the rest of the settings intact.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the *environment* cannot produce a usable
    /// configuration. That is a deployment mistake: the panel neither caused it
    /// nor can fix it, so it fails loudly.
    pub fn load_with_overrides(
        overrides: &BTreeMap<String, serde_json::Value>,
    ) -> Result<(Self, Vec<RejectedSetting>), ConfigError> {
        // Only what this binary would accept from the panel in the first place.
        // A row can outlive the rule that admitted it.
        let mut candidates = crate::settings::admitted_only(overrides.clone());
        let mut rejected = Vec::new();

        loop {
            match Self::extract_with(&candidates) {
                Ok(config) => return Ok((config, rejected)),
                Err(error) => {
                    // Which key to drop. `None` means no override caused this,
                    // so dropping more of them will not help.
                    let Some(key) = Self::blamed_key(&error, &candidates) else {
                        // No override left to blame, so re-run with none. A
                        // failure now is the deployment's, and the caller
                        // should hear about it.
                        if candidates.is_empty() {
                            return Err(error);
                        }
                        for (key, value) in std::mem::take(&mut candidates) {
                            rejected.push(RejectedSetting {
                                key,
                                value,
                                reason: error.to_string(),
                            });
                        }
                        continue;
                    };
                    let value = candidates.remove(&key).unwrap_or(serde_json::Value::Null);
                    rejected.push(RejectedSetting {
                        key,
                        value,
                        reason: error.to_string(),
                    });
                }
            }
        }
    }

    /// The environment layer, on its own, so callers can build the same stack.
    pub(crate) fn env_provider() -> Env {
        Env::prefixed(ENV_PREFIX)
            .split(ENV_NESTED_SEPARATOR)
            .ignore(NON_CONFIG_ENV_KEYS)
    }

    /// One pass: defaults, file, environment, then the overrides.
    fn extract_with(overrides: &BTreeMap<String, serde_json::Value>) -> Result<Self, ConfigError> {
        let figment = Self::defaults().merge(Self::env_provider());
        // Dotted keys, one at a time, so `share.code_length` lands where a
        // nested provider would put it without the rest of `share`.
        let figment = overrides.iter().fold(figment, |figment, (key, value)| {
            figment.merge((key.clone(), value.clone()))
        });

        let config: Self = figment.extract().map_err(Box::new)?;
        config.validate()?;
        Ok(config)
    }

    /// Which override, if any, this failure can be pinned on.
    ///
    /// Two shapes of failure. A type error comes from figment and names the
    /// path it could not deserialise; a range error comes from
    /// [`Config::validate`] and names the field. Both give a dotted key in the
    /// same spelling the overrides are stored under, so this is a lookup.
    fn blamed_key(
        error: &ConfigError,
        candidates: &BTreeMap<String, serde_json::Value>,
    ) -> Option<String> {
        match error {
            ConfigError::Invalid { field, .. } => candidates
                .contains_key(*field)
                .then(|| (*field).to_owned())
                // `auth.hashing` names a group when the rule spans two of its
                // fields. Anything stored under it is a candidate.
                .or_else(|| {
                    candidates
                        .keys()
                        .find(|key| key.starts_with(&format!("{field}.")))
                        .cloned()
                }),
            // Names a backend that was not compiled in. Nothing an override
            // set can be the cause, and dropping settings will not help.
            ConfigError::MissingFeature { .. } => None,
            ConfigError::Source(source) => source
                .clone()
                .into_iter()
                // An error with no path is about the document as a whole,
                // not about one key, so there is nothing to blame it on.
                .filter(|one| !one.path.is_empty())
                .map(|one| one.path)
                .map(|path| path.join("."))
                .find(|path| candidates.contains_key(path)),
        }
    }
}

/// A stored setting this binary would not honour, and why.
///
/// Returned as well as logged, so the settings screen can show which changes
/// did not survive. A value that vanishes into a log line just gets set again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedSetting {
    /// The dotted key.
    pub key: String,
    /// What was stored.
    pub value: serde_json::Value,
    /// What the loader said about it.
    pub reason: String,
}

impl OgConfig {
    /// Rejects a card layout this binary will not draw.
    ///
    /// Here, not at the settings endpoint, so the environment and the admin
    /// panel are held to one rule. A layout the panel refuses should not become
    /// settable by editing the environment and restarting.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] describing what about the layout is not allowed.
    fn validate(&self) -> Result<(), ConfigError> {
        #[cfg(feature = "og-images")]
        if !self.layout.trim().is_empty()
            && let Err(refused) = crate::og::layout::check(&self.layout)
        {
            return Err(ConfigError::Invalid {
                field: "og.layout",
                reason: refused.to_string(),
            });
        }
        Ok(())
    }
}

impl Config {
    /// Rejects any configuration this binary cannot honour.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError::Invalid`] for out-of-range or unparsable values and
    /// [`ConfigError::MissingFeature`] when a backend was not compiled in.
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.server.validate()?;
        self.database.validate()?;
        self.features.validate()?;
        self.auth.validate()?;
        self.share.validate()?;
        self.telemetry.validate()?;
        self.og.validate()?;
        self.scan.validate()?;
        self.site.validate()?;
        self.speed.validate()?;
        self.mail.validate()
    }
}

impl MailConfig {
    /// Refuses a half-filled mail block.
    ///
    /// Half-configured is worse than empty. Empty means the feature does not
    /// exist and every path around it stays quiet. A host with no `from` mounts
    /// the endpoints and draws the link, then fails at the moment someone is
    /// already locked out.
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
            // All or nothing, the same question `is_configured` asks.
        } else {
            return Err(ConfigError::Invalid {
                field: "mail",
                reason: format!(
                    "set all of mail.host, mail.from and mail.public_url, or none of them \
                     (only {} given)",
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
                reason: "must start with http:// or https://; it is the base of a link \
                         somebody will click"
                    .to_owned(),
            });
        }
        if self.reset_ttl_minutes == 0 || self.reset_ttl_minutes > MAX_RESET_TTL_MINUTES {
            return Err(ConfigError::Invalid {
                field: "mail.reset_ttl_minutes",
                reason: format!(
                    "must be between 1 and {MAX_RESET_TTL_MINUTES}; past a day the link has \
                     stopped being a reset and become a second credential"
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
                     registration holds a password hash for an account that does not exist"
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

    /// How long a ticket survives unused.
    #[must_use]
    pub const fn ticket_idle(&self) -> Duration {
        Duration::from_secs(self.ticket_idle_hours.saturating_mul(3600))
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
    /// Falls back to the ceiling, the only sensible reading of "no default" on
    /// an instance that has one.
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
        self.access_log.validate()?;
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
                    "must be between 1 and {MAX_TEXT_CEILING}; text is held whole in memory and \
                     stored as one row, so a larger value makes one upload that much resident \
                     memory"
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
        // A default above the ceiling would fail every share whose caller asked
        // for no lifetime at all, with nothing to tell them why.
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
/// A bound on the setting, not the setting. An operator who wants larger pastes
/// can raise `share.max_text_bytes`, but not to a number that makes one upload a
/// memory problem for everybody else.
const MAX_TEXT_CEILING: usize = 16 * 1024 * 1024;

/// Highest inline threshold an operator may configure.
///
/// Inline content is read into memory whole, so this is the largest allocation a
/// single retrieval can be made to perform. MySQL's `MEDIUMBLOB` would hold far
/// more; this limit is about memory, not the column.
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
        // Below S3's floor, every multipart upload of more than one part fails
        // at `complete`: late, remote and hard to read.
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
        if self.data_dir.trim().is_empty() {
            return Err(ConfigError::Invalid {
                field: "server.data_dir",
                reason: "must name a directory".to_owned(),
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

impl FeatureConfig {
    fn validate(self) -> Result<(), ConfigError> {
        // An instance with neither accounts nor anonymous upload can receive
        // nothing. Better said at start-up than as a 403 on every upload.
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
        self.hashing.validate()?;
        self.throttle.validate()?;
        self.oidc.validate()?;
        self.github.validate("auth.github.scopes", "user:email")?;
        self.discord.validate("auth.discord.scopes", "email")?;
        // The policy's own bounds live in the domain crate, so the rule is the
        // same whether it arrives from a file or from a test.
        self.pwned.validate()?;
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
        // Parsed here, not in telemetry::init. A typo in the filter would
        // otherwise degrade to "no logs" long after start-up.
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

    /// Almost no caller asks for a lifetime, so the default is what almost
    /// every share gets. It must not be the ceiling.
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

    /// `0` means "no opinion", which on an instance with a ceiling reads as
    /// the ceiling.
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

    /// Otherwise every share whose caller asked for nothing fails a check they
    /// cannot act on.
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

    /// Callers wrap these in a `Jail` even when setting nothing. The jail gives
    /// a clean environment *and* takes figment's global lock. Without it a test
    /// reads whatever `TESSERA_*` a concurrently running jail set: invisible
    /// under nextest, flaky under any thread-based runner.
    fn overrides(pairs: &[(&str, serde_json::Value)]) -> BTreeMap<String, serde_json::Value> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    /// What was typed into the panel beats what the deployment brought.
    #[test]
    fn a_stored_setting_beats_the_file_and_the_environment() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("TESSERA_SHARE__CODE_LENGTH", "10");

            let (config, rejected) =
                Config::load_with_overrides(&overrides(&[("share.code_length", 12.into())]))
                    .expect("must load");

            assert_eq!(config.share.code_length, 12);
            assert!(rejected.is_empty());
            Ok(())
        });
    }

    /// And with nothing stored, the order underneath is unchanged.
    #[test]
    fn without_an_override_the_environment_still_beats_the_file() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("TESSERA_SHARE__CODE_LENGTH", "10");

            let config = Config::load().expect("must load");
            assert_eq!(config.share.code_length, 10);
            Ok(())
        });
    }

    /// If a refused value stopped the process, the panel could make the
    /// instance unbootable, and the instance has to run to offer the panel.
    #[test]
    fn a_stored_value_out_of_range_is_dropped_rather_than_fatal() {
        figment::Jail::expect_with(|_jail| {
            let (config, rejected) = Config::load_with_overrides(
                // 2 is below `RetrievalCode::MIN_LENGTH`; validate() refuses it.
                &overrides(&[("share.code_length", 2.into())]),
            )
            .expect("a bad override must not stop the process");

            assert_eq!(
                config.share.code_length,
                Config::default().share.code_length,
                "it should have fallen back to the layer below"
            );
            assert_eq!(rejected.len(), 1);
            assert_eq!(rejected[0].key, "share.code_length");
            Ok(())
        });
    }

    /// A value figment cannot deserialise at all. Same rule, different failure.
    #[test]
    fn a_stored_value_of_the_wrong_type_is_dropped_too() {
        figment::Jail::expect_with(|_jail| {
            let (config, rejected) = Config::load_with_overrides(&overrides(&[(
                "share.code_length",
                "not a number".into(),
            )]))
            .expect("a bad override must not stop the process");

            assert_eq!(
                config.share.code_length,
                Config::default().share.code_length
            );
            assert_eq!(rejected.len(), 1, "{rejected:?}");
            Ok(())
        });
    }

    /// One bad setting should cost that setting, not the whole configuration.
    #[test]
    fn one_bad_setting_does_not_take_the_good_ones_with_it() {
        figment::Jail::expect_with(|_jail| {
            let (config, rejected) = Config::load_with_overrides(&overrides(&[
                ("share.code_length", 2.into()),
                ("share.ticket_ttl_hours", 3.into()),
                ("features.anonymous_upload", false.into()),
            ]))
            .expect("must load");

            assert_eq!(rejected.len(), 1, "only the bad one: {rejected:?}");
            assert_eq!(config.share.ticket_ttl_hours, 3);
            assert!(!config.features.anonymous_upload);
            Ok(())
        });
    }

    /// A row can outlive the rule that admitted it. A key can move onto the
    /// secret list, or be written straight into the table by hand. Neither may
    /// set something the panel would refuse.
    #[test]
    fn a_stored_setting_the_panel_would_refuse_is_ignored() {
        figment::Jail::expect_with(|_jail| {
            let (config, rejected) = Config::load_with_overrides(&overrides(&[
                ("server.port", 9999.into()),
                ("auth.admin_password", "hunter2 hunter2 hunter2".into()),
            ]))
            .expect("must load");

            assert_eq!(
                config.server.port,
                Config::default().server.port,
                "a boot-time setting must not be settable from the table"
            );
            assert!(
                config.auth.admin_password.is_empty(),
                "a secret must not be"
            );
            assert!(
                rejected.is_empty(),
                "these are filtered before loading, not rejected by it"
            );
            Ok(())
        });
    }

    #[rstest]
    #[case::zero_port("server.port", |c: &mut Config| c.server.port = 0)]
    #[case::zero_timeout("server.request_timeout_secs", |c: &mut Config| {
        c.server.request_timeout_secs = 0;
    })]
    #[case::tiny_memory("auth.hashing.memory_kib", |c: &mut Config| c.auth.hashing.memory_kib = 1024)]
    #[case::no_iterations("auth.hashing", |c: &mut Config| c.auth.hashing.iterations = 0)]
    #[case::no_throttle_window("auth.throttle.login", |c: &mut Config| {
        c.auth.throttle.login.window_secs = 0;
    })]
    #[case::no_throttle_attempts("auth.throttle.source", |c: &mut Config| {
        c.auth.throttle.source.max_attempts = 0;
    })]
    // An attacker fills this map, so above the ceiling the bound stops being a
    // limit and becomes the size of the hole.
    #[case::unbounded_keys("auth.throttle.max_tracked_keys", |c: &mut Config| {
        c.auth.throttle.max_tracked_keys = MAX_TRACKED_KEYS_CEILING + 1;
    })]
    #[case::small_part("server.part_size_bytes", |c: &mut Config| c.server.part_size_bytes = 1024)]
    #[case::zero_stall("server.upload_stall_secs", |c: &mut Config| c.server.upload_stall_secs = 0)]
    #[case::zero_body("server.max_body_bytes", |c: &mut Config| c.server.max_body_bytes = 0)]
    #[case::bad_origin("server.cors_allowed_origins", |c: &mut Config| {
        c.server.cors_allowed_origins = vec!["example.com".to_owned()];
    })]
    #[case::zero_pool("database.max_connections", |c: &mut Config| c.database.max_connections = 0)]
    #[case::bad_url("database.url", |c: &mut Config| c.database.url = "mongodb://x".to_owned())]
    #[case::empty_data_dir("server.data_dir", |c: &mut Config| c.server.data_dir = "  ".to_owned())]
    // A log on with no retention keeps nothing while reading as if it keeps
    // something.
    #[case::log_without_retention("share.access_log.retention_days", |c: &mut Config| {
        c.share.access_log.enabled = true;
        c.share.access_log.retention_days = 0;
    })]
    #[case::short_code("share.code_length", |c: &mut Config| c.share.code_length = 2)]
    #[case::long_code("share.code_length", |c: &mut Config| c.share.code_length = 64)]
    #[case::huge_inline("share.inline_threshold_bytes", |c: &mut Config| {
        c.share.inline_threshold_bytes = 8 * 1024 * 1024;
    })]
    #[case::zero_ticket("share.ticket_ttl_hours", |c: &mut Config| c.share.ticket_ttl_hours = 0)]
    #[case::bad_filter("telemetry.filter", |c: &mut Config| {
        c.telemetry.filter = "=====".to_owned();
    })]
    // Half a mail block mounts the endpoints and draws the link, then fails at
    // the moment someone is already locked out.
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
    // Past a day the link has become a second credential, kept somewhere less
    // protected than a password.
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
    fn environment_variables_override_defaults() {
        figment::Jail::expect_with(|jail| {
            // Nested keys use the `__` separator; scalars and lists both parse.
            jail.set_env("TESSERA_SERVER__PORT", "9999");
            jail.set_env("TESSERA_FEATURES__DOCS", "false");
            jail.set_env("TESSERA_DATABASE__URL", "sqlite://custom.db");
            jail.set_env("TESSERA_TELEMETRY__FILTER", "warn");

            let config = Config::load().expect("valid configuration");

            assert_eq!(config.server.port, 9999);
            assert!(!config.features.docs);
            assert_eq!(config.database.url, "sqlite://custom.db");
            assert_eq!(config.telemetry.filter, "warn");
            // Untouched keys keep their defaults.
            assert_eq!(config.server.request_timeout_secs, 30);
            Ok(())
        });
    }

    #[test]
    fn an_invalid_environment_override_fails_at_load_time() {
        figment::Jail::expect_with(|jail| {
            // `EnvFilter` treats most junk as a bare target name. This is one
            // of the few shapes it genuinely refuses.
            jail.set_env("TESSERA_TELEMETRY__FILTER", "=====");
            let err = Config::load().expect_err("must be rejected");
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

    /// Regression test. `TESSERA_BUILD_REV` is exported into every `cargo run`
    /// by the build script, and `TESSERA_CONFIG_FILE` is what an instance that
    /// predates the panel still has set. Both sit inside the settings
    /// namespace, so `deny_unknown_fields` refused to start; for the second
    /// that turns an upgrade into an outage.
    #[rstest]
    #[case("TESSERA_CONFIG_FILE", "/etc/tessera.toml")]
    #[case("TESSERA_BUILD_REV", "v0.1.0-3-gabcdef")]
    fn namespace_neighbours_are_not_mistaken_for_settings(#[case] key: &str, #[case] value: &str) {
        figment::Jail::expect_with(|jail| {
            jail.set_env(key, value);
            let config = Config::load()
                .unwrap_or_else(|e| panic!("{key} must not be read as a setting: {e}"));
            assert_eq!(config, Config::default());
            Ok(())
        });
    }

    /// Both ends of the all-or-nothing rule: whole and absent.
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

    /// Neither accounts nor anonymous uploads means nothing can be received.
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

    /// The spelling an operator reaches for first has to work: the way out of a
    /// setting that will not load is the boolean that turns the throttles off.
    #[test]
    fn a_comma_separated_list_is_understood() {
        figment::Jail::expect_with(|jail| {
            jail.set_env("TESSERA_SERVER__TRUSTED_PROXIES", "127.0.0.1, ::1");
            let config = Config::load().expect("must load");
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
        // 127.0.0.1 is outside the list, so it is not a proxy and gets no say.
        // The boolean on its own would have granted it one.
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
            // A typo must stop the process, not be dropped.
            jail.set_env("TESSERA_SERVER__PROT", "9001");
            let err = Config::load().expect_err("must be rejected");
            assert!(matches!(err, ConfigError::Source(_)), "got {err}");
            Ok(())
        });
    }

    /// A code length the domain type refuses fails every generated code, which
    /// is better said at start-up than on the first upload.
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
