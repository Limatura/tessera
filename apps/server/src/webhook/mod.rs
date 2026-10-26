//! Webhooks: telling a URL when a share is created, downloaded or expires.
//!
//! An event is written to a queue when it happens, one row per webhook told,
//! and [`worker`] sends what is queued. So a receiver that is down gets it
//! later, and a request never waits on one.
//!
//! # Who hears what
//!
//! The instance's webhooks, set up by an administrator, hear about every share.
//! An account's own hear about its own shares. Only the instance's are told
//! who downloaded something, and only while `share.access_log.enabled` is on:
//! that is the switch for recording people at all, and only administrators can
//! see that record.
//!
//! # The body
//!
//! JSON shaped like [Standard Webhooks](https://www.standardwebhooks.com):
//! `type`, `timestamp` and `data`, signed the way it describes. Or, for a
//! webhook in the `discord` format, a message a Discord channel will post.

pub mod address;
pub mod worker;

use serde_json::{Value, json};
use time::OffsetDateTime;

use crate::{
    api::tokens::format_timestamp,
    db::repository::{
        share::{Content, Share},
        webhook::{Event, Format, Webhook},
    },
    state::AppState,
};

/// Who retrieved a share, as far as the request says.
#[derive(Debug, Clone, Copy)]
pub struct Retriever<'a> {
    /// The address it came from, when there is an honest one.
    pub address: Option<&'a str>,
    /// What the client called itself.
    pub user_agent: Option<&'a str>,
}

/// Queues `event` on `share` for every webhook listening, and wakes the
/// worker.
///
/// A failure is logged and goes no further. The upload or download it is
/// about has already happened, and refusing it over a notice would be worse
/// than the notice going missing.
pub async fn announce(
    state: &AppState,
    event: Event,
    share: &Share,
    retriever: Option<Retriever<'_>>,
) {
    let hooks = match state.webhooks.listening(event, share.user_id).await {
        Ok(hooks) if hooks.is_empty() => return,
        Ok(hooks) => hooks,
        Err(error) => {
            tracing::warn!(%error, share = %share.id, "could not read the webhooks to tell");
            return;
        }
    };
    let content = match state.shares.content_of(share).await {
        Ok(content) => content,
        Err(error) => {
            tracing::warn!(%error, share = %share.id, "could not read a share for its webhooks");
            return;
        }
    };
    let (url, recording) = {
        let config = state.config();
        (
            link(&config.server.public_url, share),
            config.share.access_log.enabled,
        )
    };
    let message = Message {
        event,
        share,
        content: &content,
        url: url.as_deref(),
        retriever: retriever.filter(|_| recording),
        at: OffsetDateTime::now_utc(),
    };
    queue(state, &hooks, &message).await;
    state.webhook_wake.notify_one();
}

/// Writes one row per webhook. Only the instance's are told who it was.
async fn queue(state: &AppState, hooks: &[Webhook], message: &Message<'_>) {
    for hook in hooks {
        let message = Message {
            retriever: message.retriever.filter(|_| hook.owner.is_none()),
            ..*message
        };
        let body = render(hook, &message);
        if let Err(error) = state
            .webhooks
            .enqueue(hook.id, message.event, body, message.at)
            .await
        {
            tracing::warn!(%error, webhook = %hook.id, "could not queue a webhook event");
        }
    }
}

/// The share's address, when this instance knows its own.
fn link(public_url: &str, share: &Share) -> Option<String> {
    let base = public_url.trim_end_matches('/');
    (!base.is_empty()).then(|| format!("{base}/s/{}", share.code))
}

/// Everything a body is made from.
#[derive(Clone, Copy)]
struct Message<'a> {
    event: Event,
    share: &'a Share,
    content: &'a Content,
    url: Option<&'a str>,
    retriever: Option<Retriever<'a>>,
    at: OffsetDateTime,
}

fn render(hook: &Webhook, message: &Message<'_>) -> String {
    match hook.format {
        Format::Json => json_body(message),
        Format::Discord => discord_body(message),
    }
    .to_string()
}

fn json_body(message: &Message<'_>) -> Value {
    let Message {
        event,
        share,
        content,
        url,
        retriever,
        at,
    } = message;
    let mut data = json!({
        "id": share.id.to_string(),
        "code": share.code.as_str(),
        "url": url,
        "kind": share.kind.as_str(),
        "title": share.title,
        "encrypted": share.encryption.is_some(),
        "size_bytes": content.size_bytes(),
        "files": content
            .files()
            .iter()
            .map(|file| json!({ "name": file.filename, "size_bytes": file.size_bytes }))
            .collect::<Vec<_>>(),
        "use_count": share.use_count,
        "use_limit": share.use_limit,
        "expires_at": share.expires_at.map(format_timestamp),
        "owner_id": share.user_id.map(|id| id.to_string()),
    });
    if let (Some(retriever), Some(object)) = (retriever, data.as_object_mut()) {
        object.insert(
            "download".to_owned(),
            json!({ "address": retriever.address, "user_agent": retriever.user_agent }),
        );
    }
    json!({
        "type": event.as_str(),
        "timestamp": format_timestamp(*at),
        "data": data,
    })
}

/// A Discord message: one embed saying what happened to which share.
fn discord_body(message: &Message<'_>) -> Value {
    let Message {
        event,
        share,
        content,
        url,
        retriever,
        at,
    } = message;
    let title = match event {
        Event::Created => "Share created",
        Event::Downloaded => "Share downloaded",
        Event::Expired => "Share expired",
    };
    let mut fields = vec![json!({
        "name": "Size",
        "value": size(content.size_bytes()),
        "inline": true,
    })];
    let uses = match share.use_limit {
        Some(limit) => format!("{} of {limit}", share.use_count),
        None => share.use_count.to_string(),
    };
    fields.push(json!({ "name": "Downloads", "value": uses, "inline": true }));
    if let Some(address) = retriever.and_then(|retriever| retriever.address) {
        fields.push(json!({ "name": "From", "value": address, "inline": true }));
    }
    json!({
        "username": "Tessera",
        "embeds": [{
            "title": title,
            "description": format!("{}\n`{}`", name(share, content), share.code.as_str()),
            "url": url,
            "timestamp": format_timestamp(*at),
            "fields": fields,
        }],
    })
}

/// What to call a share in a message.
fn name(share: &Share, content: &Content) -> String {
    if share.encryption.is_some() {
        return "End-to-end encrypted".to_owned();
    }
    if let Some(title) = &share.title {
        return title.clone();
    }
    match content {
        Content::Files(files) => match files.as_slice() {
            [file] => file.filename.clone().unwrap_or_else(|| "A file".to_owned()),
            many => format!("{} files", many.len()),
        },
        Content::Text(_) => "A note".to_owned(),
        Content::Gone => share.kind.as_str().to_owned(),
    }
}

/// A byte count as a person reads it.
fn size(bytes: u64) -> String {
    let mut scale: u64 = 1;
    let mut units = ["KB", "MB", "GB", "TB"].into_iter();
    let mut unit = None;
    while bytes / scale >= 1024 {
        let Some(next) = units.next() else { break };
        scale = scale.saturating_mul(1024);
        unit = Some(next);
    }
    let Some(unit) = unit else {
        return format!("{bytes} B");
    };
    let tenths = u128::from(bytes) * 10 / u128::from(scale);
    format!("{}.{} {unit}", tenths / 10, tenths % 10)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_like_sizes() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(1536), "1.5 KB");
        assert_eq!(size(5 * 1024 * 1024 * 1024), "5.0 GB");
    }
}
