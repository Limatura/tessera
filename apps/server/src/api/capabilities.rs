//! What this instance can actually do.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use tessera_core::Permission;
use utoipa::ToSchema;

use crate::{
    api::{
        SYSTEM_TAG, gate,
        shares::{largest_placeable, max_file_bytes, personal_room, remaining_room},
    },
    auth::extract::MaybeUser,
    config::{FeatureConfig, RegistrationPolicy},
    error::Result,
    state::AppState,
};

/// Body of `GET /api/v1/capabilities`.
///
/// # Why this endpoint exists
///
/// `openapi.json` is a build artefact describing every endpoint the software
/// has. Which of them an *instance* exposes is a runtime decision its operator
/// made, so a single-page app generated from the contract cannot know whether
/// to render a login button: the contract says login exists, and on this
/// deployment it may not.
///
/// So the contract describes the whole surface and this reports the subset in
/// force. It is unauthenticated, because a client needs the answer before it
/// can know whether logging in is possible at all.
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
    /// `false` means no registration, no login and no session: the shape for a
    /// personal instance behind a VPN.
    pub accounts: bool,

    /// Whether the administrative endpoints are mounted.
    ///
    /// The administration front-end is a separate deployment, so this says only
    /// that the API half is present.
    pub admin: bool,

    /// Whether a caller without a session may upload.
    ///
    /// Independent of [`accounts`](Self::accounts): an instance can have
    /// accounts *and* accept anonymous uploads, the default.
    pub anonymous_upload: bool,

    /// Who may create an account. Meaningless when
    /// [`accounts`](Self::accounts) is `false`.
    #[schema(example = "open")]
    pub registration: RegistrationPolicy,

    /// Shortest *account* password this instance accepts, so a client can say so
    /// before the round trip, not after it.
    ///
    /// Named for what it governs, because there are two minimums and they
    /// differ. See
    /// [`share_password_min_length`](Self::share_password_min_length).
    #[schema(example = 12)]
    pub account_password_min_length: usize,

    /// Shortest password an uploader may put on a share.
    ///
    /// Lower than the account minimum by design. A share password is a second
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
    /// control instead of offering a date the server will refuse.
    #[schema(example = 8760_u64)]
    pub max_expiry_hours: Option<u64>,

    /// Whether registering sends a confirmation link instead of creating the
    /// account outright.
    ///
    /// `true` means `POST /auth/register` answers `202` and nothing exists
    /// until the address is confirmed, so a client must show "check your inbox"
    /// instead of treating the response as a sign-in. `false` means it answers
    /// `201` with a session, as an instance sending no mail does.
    pub registration_requires_confirmation: bool,

    /// Whether this instance can send a password reset link.
    ///
    /// `false` when no relay is configured, the default. A client that
    /// drew the "forgot password" link regardless would send people to an
    /// endpoint answering 404, so this is the difference between a missing
    /// feature and a broken one.
    pub password_reset: bool,

    /// Whether this instance writes down who fetches a share.
    ///
    /// `false` by default. When `true`, every retrieval records the visitor's
    /// address and browser string, and the visitor is the person it happens to,
    /// so a client should say so on the page where the share is collected.
    /// Nothing else will.
    pub access_log: bool,

    /// Largest single file *this caller* may send, in bytes.
    ///
    /// The narrowest of their own limit, their role's and
    /// `server.max_body_bytes`. So an operator who raised the instance figure
    /// for themselves does not have it reported to every stranger, and a client
    /// is never shown a number bigger than what it may send.
    #[schema(example = 104_857_600_u64)]
    pub max_upload_bytes: u64,

    /// How much room the caller has left, in bytes.
    ///
    /// Whichever ceiling refuses them first: their own quota, then their
    /// role's. Callers who may read instance statistics also see the instance
    /// ceiling. `null` if none applies.
    ///
    /// `storage.max_total_bytes` is not in everybody's answer because on a
    /// public instance it is the operator's capacity planning, and telling
    /// every anonymous uploader how close the disk is to full invites them to
    /// finish it off. Their own quota is theirs to know, and a client holding
    /// it can say so before they pick a file.
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
        let config = state.config();
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
            access_log: config.share.access_log.enabled,
            // `usize` is the type axum's body limit wants and `u64` is the type
            // a file size is. The conversion cannot fail on any target this
            // runs on, but saturating still beats a cast that would wrap.
            //
            // The instance figure. `capabilities` narrows it to the caller's
            // own before answering. A client shown a number bigger than what it
            // may send will offer a file picker that then fails.
            max_upload_bytes: u64::try_from(config.server.max_body_bytes).unwrap_or(u64::MAX),
            storage_remaining_bytes: None,
            version: crate::VERSION.to_owned(),
        }
    }
}

/// Reports which optional parts of the service this instance exposes.
///
/// Answers to everyone, signed in or not, since a client needs this before it
/// can know whether logging in is possible. An administrator gets one extra
/// field; see [`Capabilities::storage_remaining_bytes`].
///
/// # Errors
///
/// Only when the storage total cannot be read: an administrator's
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

    // Whichever ceiling would refuse this caller first. A caller who may read
    // instance-wide figures is told about the disk as well as their own quota;
    // everyone else only about theirs, because telling every anonymous
    // uploader how close the disk is to full invites them to finish it off.
    //
    // Tightest, not instance-only, even for an administrator, since one
    // with a quota of their own would otherwise be shown 900 GB free and
    // refused at four kilobytes.
    let user = current.as_ref().map(|current| &current.user);
    capabilities.max_upload_bytes = max_file_bytes(&state, user).await?;
    let may_see_totals = current
        .as_ref()
        .is_some_and(|current| current.permissions.allows(Permission::StatsRead));
    let room = if may_see_totals {
        remaining_room(&state, user).await?
    } else {
        personal_room(&state, user).await?
    };
    // `u64::MAX` means nothing limits them, and that is not a figure to send.
    if room.bytes != u64::MAX {
        capabilities.storage_remaining_bytes = Some(room.bytes);
    }

    // A quota can say there is room that no single backend can actually take:
    // nothing is split, so a pool of five holding 100 MB each refuses a 200 MB
    // file however much quota is left. Whichever answer is smaller is the one a
    // client can act on.
    if let Some(placeable) = largest_placeable(&state).await? {
        capabilities.max_upload_bytes = capabilities.max_upload_bytes.min(placeable);
        capabilities.storage_remaining_bytes = Some(
            capabilities
                .storage_remaining_bytes
                .map_or(placeable, |left| left.min(placeable)),
        );
    }

    Ok(Json(capabilities))
}
