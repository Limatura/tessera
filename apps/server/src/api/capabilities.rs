//! What this instance can actually do.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use tessera_core::Permission;
use utoipa::ToSchema;

use crate::{
    api::{SYSTEM_TAG, gate},
    auth::extract::MaybeUser,
    config::{FeatureConfig, RegistrationPolicy},
    error::Result,
    state::AppState,
};

/// Body of `GET /api/v1/capabilities`.
///
/// # Why this endpoint exists
///
/// `openapi.json` is a build artefact: it describes every endpoint the software
/// has. Which of them an *instance* exposes is a runtime decision its operator
/// made. A single-page app generated from the contract therefore cannot know
/// whether to render a login button — the contract says login exists, and on
/// this deployment it may not.
///
/// So the contract describes the whole surface and this reports the subset in
/// force. It is deliberately unauthenticated: a client needs the answer before
/// it can know whether logging in is even possible.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
// Each of these is an independent yes/no about the instance, and that is what
// the endpoint is for. Grouping them into an enum or a bitfield to satisfy the
// count would make the JSON worse for the clients that read it.
#[expect(
    clippy::struct_excessive_bools,
    reason = "a capability report is a list of independent switches"
)]
pub struct Capabilities {
    /// Whether accounts exist at all on this instance.
    ///
    /// `false` means there is no registration, no login and no session — the
    /// shape for a personal instance behind a VPN.
    pub accounts: bool,

    /// Whether the administrative endpoints are mounted.
    ///
    /// The administration front-end is a separate deployment, so this says only
    /// that the API half is present.
    pub admin: bool,

    /// Whether a caller without a session may upload.
    ///
    /// Independent of [`accounts`](Self::accounts): an instance can have
    /// accounts *and* accept anonymous uploads, which is the default.
    pub anonymous_upload: bool,

    /// Who may create an account. Meaningless when
    /// [`accounts`](Self::accounts) is `false`.
    #[schema(example = "open")]
    pub registration: RegistrationPolicy,

    /// Shortest *account* password this instance accepts, so a client can say so
    /// before the round trip rather than after it.
    ///
    /// Named for what it governs because there are two minimums and they are
    /// deliberately different — see
    /// [`share_password_min_length`](Self::share_password_min_length).
    #[schema(example = 12)]
    pub account_password_min_length: usize,

    /// Shortest password an uploader may put on a share.
    ///
    /// Lower than the account minimum on purpose: a share password is a second
    /// factor on a link that is already unguessable, and it gets read out over
    /// the phone. A form that validated share passwords against the account
    /// rule would refuse the four-digit code the server would have accepted.
    #[schema(example = 4)]
    pub share_password_min_length: usize,

    /// How long a share lasts when the uploader does not choose, in hours.
    ///
    /// The upload form needs this to say what "leave it alone" means. `null`
    /// when this instance lets shares live forever by default.
    #[schema(example = 168_u64)]
    pub default_expiry_hours: Option<u64>,

    /// The longest lifetime an uploader may ask for, in hours.
    ///
    /// `null` when there is no ceiling. A form that knows this can bound its own
    /// control instead of letting somebody pick a date the server will refuse.
    #[schema(example = 8760_u64)]
    pub max_expiry_hours: Option<u64>,

    /// Whether registering sends a confirmation link rather than creating the
    /// account outright.
    ///
    /// `true` means `POST /auth/register` answers `202` and nothing exists
    /// until the address is confirmed — so a client must show "check your
    /// inbox" rather than treating the response as a sign-in. `false` means it
    /// answers `201` with a session, which is what an instance that sends no
    /// mail does.
    pub registration_requires_confirmation: bool,

    /// Whether this instance can send a password reset link.
    ///
    /// `false` when no relay is configured, which is the default. A client that
    /// draws the "forgot password" link regardless would send people to an
    /// endpoint that answers 404 — so this is the difference between a missing
    /// feature and a broken one.
    pub password_reset: bool,

    /// Longest accepted request body, in bytes.
    #[schema(example = 104_857_600_u64)]
    pub max_upload_bytes: u64,

    /// How much of the instance's storage quota is left, in bytes.
    ///
    /// `null` for everyone but an administrator, and also for an administrator
    /// when the instance has no quota set.
    ///
    /// Only an administrator, because on a public instance this is the
    /// operator's capacity planning and not the visitor's business: telling
    /// every anonymous uploader how close the disk is to full invites somebody
    /// to close the gap. An uploader who exceeds it gets a `507` naming the
    /// limit, which is the moment they actually need to know.
    #[schema(example = 53_687_091_200_u64)]
    pub storage_remaining_bytes: Option<u64>,

    /// Version of the running server, for a client that wants to warn about a
    /// mismatch with the contract it was generated from.
    #[schema(example = "0.1.0")]
    pub version: String,
}

/// Seconds in an hour, for reporting durations in the unit the API takes them
/// in.
const SECONDS_PER_HOUR: u64 = 60 * 60;

impl Capabilities {
    /// Derives the effective answer from configuration.
    ///
    /// `storage_remaining_bytes` is left `None` here and filled in by the
    /// handler, because it is the one field that depends on who is asking.
    fn from_state(state: &AppState) -> Self {
        let config = &state.config;
        let FeatureConfig {
            accounts,
            admin,
            anonymous_upload,
            docs: _,
        } = config.features;

        // The accessors and not the raw fields: `default_expiry_hours` of 0
        // means "as long as the ceiling allows", so the raw number is not the
        // answer to "what happens if I leave this alone".
        let hours = |duration: Option<std::time::Duration>| {
            duration.map(|duration| duration.as_secs() / SECONDS_PER_HOUR)
        };

        Self {
            accounts,
            admin,
            anonymous_upload,
            registration: config.auth.registration,
            account_password_min_length: config.auth.password.min_length,
            share_password_min_length: gate::CHOOSING.min_length,
            default_expiry_hours: hours(config.share.default_expiry()),
            max_expiry_hours: hours(config.share.max_expiry()),
            registration_requires_confirmation: config.mail.is_configured(),
            password_reset: config.mail.is_configured(),
            // `usize` is the type axum's body limit wants; `u64` is the type a
            // file size is. The conversion cannot fail on any target this runs
            // on, but saturating is still better than a cast that would wrap.
            max_upload_bytes: u64::try_from(config.server.max_body_bytes).unwrap_or(u64::MAX),
            storage_remaining_bytes: None,
            version: crate::VERSION.to_owned(),
        }
    }
}

/// Reports which optional parts of the service this instance exposes.
///
/// Answers to everyone, signed in or not — a client needs this before it can
/// know whether logging in is even possible. An administrator gets one extra
/// field; see [`Capabilities::storage_remaining_bytes`].
///
/// # Errors
///
/// Only when the storage total cannot be read, which is an administrator's
/// request against a database that is not answering.
#[utoipa::path(
    get,
    path = "/capabilities",
    tag = SYSTEM_TAG,
    responses(
        (status = 200, description = "What this instance exposes", body = Capabilities),
    ),
    security(("session" = []), ("bearer" = []), ()),
)]
pub async fn capabilities(
    State(state): State<AppState>,
    MaybeUser(current): MaybeUser,
) -> Result<Json<Capabilities>> {
    let mut capabilities = Capabilities::from_state(&state);

    let quota = state.config.storage.max_total_bytes;
    // The remaining space is an instance-wide number, so it is told to whoever
    // may read instance-wide numbers — not to whoever happens to be called an
    // administrator.
    let may_see_totals =
        current.is_some_and(|current| current.permissions.allows(Permission::StatsRead));
    if may_see_totals && quota != 0 {
        let used = state.shares.total_stored_bytes().await?;
        capabilities.storage_remaining_bytes = Some(quota.saturating_sub(used));
    }

    Ok(Json(capabilities))
}
