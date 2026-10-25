//! Sends what is queued, and announces the shares that have ended.
//!
//! Woken when an event is queued, and every thirty seconds regardless, which is
//! how a retry comes due and how an expiry is noticed: a share has no moment
//! of expiring, only a time after which it has.
//!
//! # Retries
//!
//! Anything but a `2xx` answer is a failure, and so is no answer within ten
//! seconds. A failed event is sent again after 30 seconds, 2 minutes, 10
//! minutes, an hour, 6 hours and a day, then given up on. Each attempt goes out
//! under the same `webhook-id`, so a receiver can tell a retry from a new
//! event.

use std::time::Duration;

use aws_lc_rs::hmac;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use time::OffsetDateTime;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

use super::address;
use crate::{
    db::repository::webhook::{Due, Event, Outcome},
    state::AppState,
};

/// How often the queue is checked when nothing has woken it.
const INTERVAL: Duration = Duration::from_secs(30);

/// How long a receiver has to answer.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How many events are being sent at once, so one slow receiver does not hold
/// up the rest.
const PARALLEL: usize = 4;

/// How many queued events are read at a time.
const BATCH: u64 = 32;

/// How many batches one pass sends before it lets the timer decide again.
const BATCHES_PER_PASS: usize = 16;

/// How many ended shares one pass announces.
const ENDS_PER_PASS: u64 = 100;

/// How long to wait after each failed attempt. One more attempt is made than
/// there are entries.
const RETRY_AFTER: [Duration; 6] = [
    Duration::from_secs(30),
    Duration::from_mins(2),
    Duration::from_mins(10),
    Duration::from_hours(1),
    Duration::from_hours(6),
    Duration::from_hours(24),
];

/// The prefix of a signing secret, as Standard Webhooks spells it.
pub const SECRET_PREFIX: &str = "whsec_";

/// The two ways out: one for an account's webhooks, limited to public
/// addresses, and one for the instance's own.
#[derive(Debug, Clone)]
pub struct Clients {
    public_only: reqwest::Client,
    any: reqwest::Client,
}

impl Clients {
    /// Builds both.
    ///
    /// # Errors
    ///
    /// Returns the client's error when TLS cannot be set up.
    pub fn new() -> Result<Self, reqwest::Error> {
        let builder = || {
            reqwest::Client::builder()
                .timeout(TIMEOUT)
                .connect_timeout(TIMEOUT)
                // A redirect is a second URL nobody checked.
                .redirect(reqwest::redirect::Policy::none())
                .user_agent(concat!("Tessera-Webhooks/", env!("CARGO_PKG_VERSION")))
        };
        Ok(Self {
            // No proxy either: a proxy resolves the name itself, past the check.
            public_only: builder()
                .no_proxy()
                .dns_resolver(std::sync::Arc::new(address::PublicOnly))
                .build()?,
            any: builder().build()?,
        })
    }
}

/// Sends until `shutdown` is cancelled.
pub async fn run(state: AppState, shutdown: CancellationToken) {
    let clients = match Clients::new() {
        Ok(clients) => clients,
        Err(error) => {
            tracing::error!(%error, "webhooks cannot be sent: the HTTP client did not build");
            return;
        }
    };
    loop {
        run_once_with(&state, &clients).await;
        tokio::select! {
            () = shutdown.cancelled() => return,
            () = state.webhook_wake.notified() => {}
            () = tokio::time::sleep(INTERVAL) => {}
        }
    }
}

/// One pass: announce what has ended, then send what is due.
///
/// Public so tests can drive it one step at a time instead of waiting on a
/// timer.
pub async fn run_once(state: &AppState) {
    match Clients::new() {
        Ok(clients) => run_once_with(state, &clients).await,
        Err(error) => {
            tracing::error!(%error, "webhooks cannot be sent: the HTTP client did not build");
        }
    }
}

async fn run_once_with(state: &AppState, clients: &Clients) {
    announce_ends(state).await;
    for _ in 0..BATCHES_PER_PASS {
        let due = match state.webhooks.due(OffsetDateTime::now_utc(), BATCH).await {
            Ok(due) if !due.is_empty() => due,
            Ok(_) => return,
            Err(error) => {
                tracing::error!(%error, "could not read the webhook queue");
                return;
            }
        };
        send_all(state, clients, due).await;
    }
}

/// Queues `share.expired` for the shares that have ended since the last pass.
async fn announce_ends(state: &AppState) {
    let now = OffsetDateTime::now_utc();
    let ended = match state.shares.find_unannounced_ends(now, ENDS_PER_PASS).await {
        Ok(ended) => ended,
        Err(error) => {
            tracing::error!(%error, "could not look for shares that have ended");
            return;
        }
    };
    for share in ended {
        super::announce(state, Event::Expired, &share, None).await;
        if let Err(error) = state.shares.mark_end_announced(share.id, now).await {
            tracing::error!(%error, share = %share.id, "could not note that a share's end was announced");
        }
    }
}

async fn send_all(state: &AppState, clients: &Clients, due: Vec<Due>) {
    let mut running = JoinSet::new();
    let mut due = due.into_iter();
    loop {
        while running.len() < PARALLEL {
            let Some(event) = due.next() else { break };
            let clients = clients.clone();
            running.spawn(async move {
                let outcome = send(&clients, &event).await;
                (event, outcome)
            });
        }
        let Some(finished) = running.join_next().await else {
            return;
        };
        let Ok((event, outcome)) = finished else {
            continue;
        };
        if let Outcome::Failed {
            error, at: None, ..
        } = &outcome
        {
            tracing::warn!(webhook = %event.webhook.id, error, "gave up on a webhook event");
        }
        if let Err(error) = state
            .webhooks
            .record(event.id, outcome, OffsetDateTime::now_utc())
            .await
        {
            tracing::error!(%error, delivery = %event.id, "could not record a webhook attempt");
        }
    }
}

/// Sends one event, once.
async fn send(clients: &Clients, event: &Due) -> Outcome {
    let now = OffsetDateTime::now_utc();
    let retry = || {
        usize::try_from(event.attempts)
            .ok()
            .and_then(|attempts| RETRY_AFTER.get(attempts))
            .map(|wait| now.saturating_add(time::Duration::try_from(*wait).unwrap_or_default()))
    };
    let hook = &event.webhook;
    let client = if hook.owner.is_some() {
        // A name is checked by the resolver; an address never reaches it.
        let allowed = url::Url::parse(&hook.url)
            .ok()
            .is_some_and(|url| address::check(&url).is_ok());
        if !allowed {
            return Outcome::Failed {
                status: None,
                error: "the URL is not a public address".to_owned(),
                at: None,
            };
        }
        &clients.public_only
    } else {
        &clients.any
    };
    let id = event.id.to_string();
    let timestamp = now.unix_timestamp();
    let response = client
        .post(&hook.url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("webhook-id", &id)
        .header("webhook-timestamp", timestamp.to_string())
        .header(
            "webhook-signature",
            sign(&hook.secret, &id, timestamp, &event.body),
        )
        .body(event.body.clone())
        .send()
        .await;
    match response {
        Ok(response) if response.status().is_success() => Outcome::Delivered {
            status: response.status().as_u16(),
        },
        Ok(response) => Outcome::Failed {
            status: Some(response.status().as_u16()),
            error: format!("answered {}", response.status()),
            at: retry(),
        },
        // Without the URL: an account's Discord URL is its credential, and the
        // error is shown back.
        Err(error) => Outcome::Failed {
            status: None,
            error: error.without_url().to_string(),
            at: retry(),
        },
    }
}

/// The `webhook-signature` header for a body, as Standard Webhooks defines it:
/// an HMAC-SHA256 over the id, the timestamp and the body, keyed with the
/// secret after its prefix.
#[must_use]
pub fn sign(secret: &str, id: &str, timestamp: i64, body: &str) -> String {
    let key = secret
        .strip_prefix(SECRET_PREFIX)
        .and_then(|encoded| BASE64.decode(encoded).ok())
        .unwrap_or_else(|| secret.as_bytes().to_vec());
    let key = hmac::Key::new(hmac::HMAC_SHA256, &key);
    let tag = hmac::sign(&key, format!("{id}.{timestamp}.{body}").as_bytes());
    format!("v1,{}", BASE64.encode(tag.as_ref()))
}

/// A new signing secret.
#[must_use]
pub fn generate_secret() -> String {
    let mut bytes = [0_u8; 24];
    crate::auth::secret::fill_random(&mut bytes);
    format!("{SECRET_PREFIX}{}", BASE64.encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example in the Standard Webhooks specification.
    #[test]
    fn signs_as_standard_webhooks_does() {
        assert_eq!(
            sign(
                "whsec_MfKQ9r8GKYqrTwjUPD8ILPZIo2LaLaSw",
                "msg_p5jXN8AQM9LWM0D4loKWxJek",
                1_614_265_330,
                r#"{"test": 2432232314}"#,
            ),
            "v1,g0hM9SsE+OTPJTGt/tmIKtSyZlE3uFJELVlNIOLJ1OE="
        );
    }

    #[test]
    fn a_secret_has_its_prefix_and_decodes() {
        let secret = generate_secret();
        let encoded = secret.strip_prefix(SECRET_PREFIX).unwrap_or_default();
        assert_eq!(BASE64.decode(encoded).map(|key| key.len()).ok(), Some(24));
    }
}
