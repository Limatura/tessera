//! The one place that decides whether a share may be handed out.
//!
//! Four routes read a share: the page, its metadata, `/raw` and `/download`.
//! They must not be able to disagree, since a `/raw` that skipped the password
//! check would make the password decorative. The checks live here, and the
//! routes differ only in what they may do with the result.
//!
//! # Authorising and spending are separate steps
//!
//! [`open`] answers "does this exist and is it live". [`spend`] is what costs
//! the share one of its permitted retrievals, and it hands back a ticket that
//! can be presented as many times as the transfer needs.
//!
//! That split is why the counting works. A download is not one request:
//! `Range` makes it several, a dropped connection several more, and a browser
//! may prefetch before anyone clicks. Counting requests would let a resumed
//! download burn all three uses of a share that allows three, and an
//! interrupted one destroy a burn-after-reading share before its reader saw it.
//!
//! # A ticket outlives the count, and only the count
//!
//! Spending the last retrieval leaves the share [`Availability::Exhausted`]
//! *before* the bytes have moved. Refusing on that would make a one-use share
//! impossible to collect, since the download that paid for it would be turned
//! away by the payment. So [`admit`] lets a held ticket through that one state,
//! the same rule that lets a resumed transfer continue.
//!
//! It does not extend to [`Availability::Expired`] or
//! [`Availability::Disabled`], and the difference is who decided. Exhaustion is
//! this reader's own retrieval coming back around; an expiry is the uploader
//! saying "not after this" and a disable is an operator saying "not any more".
//! A ticket must not outrank either, so a transfer still running when the share
//! expires stops there.

use axum::http::HeaderMap;
use tessera_core::{Password, PasswordPolicy, RetrievalCode};
use time::OffsetDateTime;

use crate::{
    auth::{password, secret::Secret, throttle::Decision},
    db::repository::share::{Availability, Share},
    error::{Error, Result},
    state::AppState,
};

/// Header carrying a share's password.
///
/// A header, not a query parameter. Query strings reach access logs,
/// shell history, and the `Referer` a page sends to whatever it links to next.
pub const PASSWORD_HEADER: &str = "x-share-password";

/// Header carrying a download authorisation.
pub const TICKET_HEADER: &str = "x-share-ticket";

/// What a share's password must satisfy to be *chosen*.
///
/// Lower than an account's minimum by design. A share password is a second
/// factor on a link that is already unguessable and often read out over the
/// phone, so a four-digit code is reasonable. An account password is the only
/// thing between a stranger and the account: a different job.
pub const CHOOSING: PasswordPolicy = PasswordPolicy {
    min_length: 4,
    max_length: PasswordPolicy::CEILING,
};

/// What a share's password must satisfy to be *presented*.
///
/// Anything non-empty, up to the ceiling. Not `PasswordPolicy::widest()`: that
/// one describes account passwords and has a floor of eight, so verifying a
/// four-character share password against it rejects the right password before
/// it ever reaches the hash.
///
/// Wider than [`CHOOSING`] for the same reason accounts have the distinction: a
/// share's password was chosen under whatever rule was in force then, and
/// tightening the rule must not make existing shares unopenable.
const PRESENTING: PasswordPolicy = PasswordPolicy {
    min_length: 1,
    max_length: PasswordPolicy::CEILING,
};

/// Whether the request arrived holding a live authorisation for this share.
///
/// A named pair, not a `bool`, because the call it feeds reads
/// `admit(&share, Ticket::Held)` at every site, and `admit(&share, true)` would
/// not say what the `true` was about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ticket {
    /// A ticket for this share, issued by [`spend`] and not yet past its TTL.
    Held,
    /// No ticket, one belonging to a different share, or one that has expired.
    Absent,
}

/// Finds a live share, or says why it is not one.
///
/// Does **not** check the password, which costs an Argon2 verification not
/// worth paying for a share that has already expired.
///
/// # Errors
///
/// `404` when nothing has that code, `410` when it existed and is finished with,
/// `429` when this source has tried too many codes that do not exist.
pub async fn open(state: &AppState, code: &str, source: &str) -> Result<Share> {
    let share = find(state, code, source).await?;
    admit(&share, Ticket::Absent)?;
    Ok(share)
}

/// Finds a share by code without judging whether it may be handed out.
///
/// Split from [`open`] for the routes that transfer content: they have to know
/// whether the caller holds a ticket before the judgement can be made, and
/// looking the ticket up needs the share. Everything else wants [`open`].
///
/// # Errors
///
/// `404` when nothing has that code, `429` when this source has tried too many
/// codes that do not exist.
pub async fn find(state: &AppState, code: &str, source: &str) -> Result<Share> {
    // Checked before the lookup, so a refused source costs a comparison rather
    // than a query.
    let throttles = &state.throttles;
    if let Decision::Deny { retry_after } = throttles.lookup.check(source) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }

    // An unparsable code and an unused one are the same answer. Distinguishing
    // them would tell someone walking the code space which shapes are worth
    // trying.
    let found = match RetrievalCode::parse(code) {
        Ok(code) => state.shares.find_by_code(&code).await?,
        Err(_) => None,
    };
    let Some(share) = found else {
        // Only a miss counts. A share that exists but has expired is a return
        // visit to a link that was given out, not a guess.
        //
        // A hit does *not* reset the count either: an enumerator holding one
        // code they know is real could otherwise clear the counter between
        // batches. The window forgives it instead, which cannot be gamed.
        throttles.lookup.record_failure(source);
        return Err(Error::NotFound);
    };
    Ok(share)
}

/// Decides whether this caller may have this share, now.
///
/// See the module docs for why a ticket clears exhaustion and nothing else.
///
/// # Errors
///
/// `404` when it is disabled, `410` when it is expired, or exhausted and the
/// caller holds no ticket for it.
pub fn admit(share: &Share, ticket: Ticket) -> Result<()> {
    match share.availability_at(OffsetDateTime::now_utc()) {
        Availability::Available => Ok(()),
        // Disabled reads as "never existed": an operator switching a share off
        // has decided it should stop being a thing, and confirming it was real
        // serves no one.
        Availability::Disabled => Err(Error::NotFound),
        Availability::Expired => Err(Error::Gone("this share has expired".to_owned())),
        // The retrieval this ticket paid for is the one that emptied the share,
        // so this is that reader collecting what they already bought.
        Availability::Exhausted if ticket == Ticket::Held => Ok(()),
        Availability::Exhausted => Err(Error::Gone(
            "this share has been retrieved as many times as it was allowed to be".to_owned(),
        )),
    }
}

/// Verifies the share's password, when it has one.
///
/// # Why this is throttled
///
/// A share password may be four characters, which suits something read out over
/// the phone and is worthless without a limit on guesses. Measured: an
/// unthrottled endpoint answered 246 attempts a second, so ten thousand
/// combinations fell in under a minute. Each attempt also costs a full Argon2
/// verification, so the same requests make the server allocate.
///
/// # Errors
///
/// `401` when the share needs a password and the request did not carry the
/// right one, `429` when too many attempts have failed.
pub async fn unlock(
    state: &AppState,
    share: &Share,
    headers: &HeaderMap,
    source: &str,
) -> Result<()> {
    let Some(stored) = share.password_hash.clone() else {
        return Ok(());
    };

    let throttles = &state.throttles;
    let key = format!("{}|{source}", share.code);
    if let Decision::Deny { retry_after } = throttles.check(&throttles.share, &key, source) {
        return Err(Error::TooManyRequests {
            retry_after_secs: retry_after.as_secs(),
        });
    }

    let presented = headers
        .get(PASSWORD_HEADER)
        .and_then(|value| value.to_str().ok())
        // See `PRESENTING`: the bounds here are wider than the ones a password
        // is chosen under, because it was chosen under whatever rule applied
        // then.
        .and_then(|raw| Password::parse(raw, PRESENTING).ok());

    let Some(presented) = presented else {
        throttles.record_failure(&throttles.share, &key, source);
        return Err(Error::Unauthorized);
    };
    if password::verify(presented, stored).await? {
        throttles.share.reset(&key);
        Ok(())
    } else {
        throttles.record_failure(&throttles.share, &key, source);
        Err(Error::Unauthorized)
    }
}

/// Spends one of the share's permitted retrievals and issues a ticket.
///
/// # Errors
///
/// `410` when the limit was reached between [`open`] and here. The count is
/// decided by the database, so two callers racing for the last retrieval get
/// one winner.
pub async fn spend(state: &AppState, share: &Share) -> Result<Secret> {
    let now = OffsetDateTime::now_utc();
    if !state.shares.consume(share.id, now).await? {
        return Err(Error::Gone(
            "this share has been retrieved as many times as it was allowed to be".to_owned(),
        ));
    }

    // Issued with the idle window, not the ceiling. The ticket is renewed by
    // being used, so starting it at its maximum leaves the renewal nothing to
    // do and the whole life becomes the fixed one this replaced.
    //
    // Still capped by the ceiling, since an instance may configure an idle
    // window longer than the total life, and the ticket must not be born past
    // the limit its renewals are checked against.
    let config = state.config();
    let first = Ord::min(
        duration_of(config.share.ticket_idle()),
        duration_of(config.share.ticket_ttl()),
    );
    let (_, secret) = state
        .tickets
        .issue(share.id, now, now.saturating_add(first))
        .await?;
    Ok(secret)
}

/// Whether the request already holds an authorisation for this share.
///
/// A ticket is bound to one share, so presenting another share's is the same as
/// presenting none.
///
/// # Errors
///
/// Returns [`Error`] if the lookup fails.
pub async fn holds_ticket(
    state: &AppState,
    share: &Share,
    headers: &HeaderMap,
    from_query: Option<&str>,
) -> Result<Ticket> {
    let Some(secret) = ticket_secret(headers, from_query) else {
        return Ok(Ticket::Absent);
    };
    let Some(ticket) = state.tickets.find_by_secret(&secret).await? else {
        return Ok(Ticket::Absent);
    };
    let now = OffsetDateTime::now_utc();
    if ticket.share_id != share.id || !ticket.is_valid_at(now) {
        return Ok(Ticket::Absent);
    }

    // Presenting it keeps it alive. A transfer still going, whether a resumed
    // download, a run of `Range` requests or a client retrying with backoff,
    // holds its authorisation as long as it needs, and one that finished is
    // gone within `share.ticket_idle_hours`.
    //
    // That is what makes a ticket not worth passing on: its value decays while
    // it sits in a message, instead of lasting the whole day a fixed lifetime
    // would have to allow for.
    //
    // A failure here is logged, not returned. The caller has a valid ticket and
    // is owed the content; refusing over a failed bookkeeping write would turn
    // a slipped expiry into an outage.
    let config = state.config();
    if let Err(error) = state
        .tickets
        .touch(
            &ticket,
            now,
            duration_of(config.share.ticket_idle()),
            duration_of(config.share.ticket_ttl()),
        )
        .await
    {
        tracing::warn!(?error, "could not extend a download authorisation");
    }
    Ok(Ticket::Held)
}

/// A `std::time::Duration` as `time`'s, saturating instead of panicking.
fn duration_of(value: std::time::Duration) -> time::Duration {
    time::Duration::try_from(value).unwrap_or(time::Duration::MAX)
}

/// Reads a ticket from the header or, failing that, the `t` query parameter.
///
/// The query form exists because a download is an `<a href>`, and a browser
/// navigation cannot carry a custom header. It is the weaker of the two, so it
/// is only consulted when the header is absent.
fn ticket_secret(headers: &HeaderMap, from_query: Option<&str>) -> Option<Secret> {
    headers
        .get(TICKET_HEADER)
        .and_then(|value| value.to_str().ok())
        .or(from_query)
        .map(Secret::from_client)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A share password is a second factor on an already-unguessable link, so it
    /// is allowed to be the sort of thing people read out over the phone.
    #[test]
    fn a_four_character_share_password_may_be_chosen() {
        assert!(Password::parse("1234", CHOOSING).is_ok());
        assert!(Password::parse("123", CHOOSING).is_err());
    }

    /// The regression this pair exists for: verifying against the *account*
    /// bounds rejects a four-character share password before it reaches the
    /// hash, so the right password answers `401`.
    #[test]
    fn a_password_that_may_be_chosen_may_also_be_presented() {
        for candidate in ["1234", "a longer passphrase", &"x".repeat(200)] {
            if Password::parse(candidate, CHOOSING).is_ok() {
                assert!(
                    Password::parse(candidate, PRESENTING).is_ok(),
                    "`{candidate}` can be set but not used"
                );
            }
        }
        assert!(
            Password::parse("1234", PasswordPolicy::widest()).is_err(),
            "the account bounds are the wrong ones here, which is the point"
        );
    }
}
