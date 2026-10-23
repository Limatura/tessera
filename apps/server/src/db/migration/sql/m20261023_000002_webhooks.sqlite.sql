-- Webhooks: URLs told when a share is created, downloaded or expires.
--
-- A webhook with no `user_id` is the instance's, set up by an administrator,
-- and hears about every share. One with a `user_id` is that account's, and
-- hears about its own shares only.
--
-- The URL and the signing secret are sealed with `security.secret_key`, like a
-- storage backend's credentials: a Discord webhook URL is itself the credential
-- for posting to that channel.
CREATE TABLE IF NOT EXISTS webhooks (
    id         TEXT NOT NULL PRIMARY KEY,
    user_id    TEXT REFERENCES users (id) ON DELETE CASCADE,
    url        TEXT NOT NULL,
    -- Space-separated, such as `share.created share.downloaded`.
    events     TEXT NOT NULL,
    -- `json`, or `discord` for a Discord channel's webhook.
    format     TEXT NOT NULL,
    secret     TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webhooks_user ON webhooks (user_id);

-- One event on its way to one webhook, and what happened when it was sent.
--
-- The body is written once, when the event happens, so a retry sends the same
-- bytes under the same id and the receiver can tell it has seen it before.
--
-- `next_attempt_at` is NULL once the event was delivered or given up on. Rows
-- are kept a while after that, so the owner can see what was sent.
CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id              TEXT    NOT NULL PRIMARY KEY,
    webhook_id      TEXT    NOT NULL REFERENCES webhooks (id) ON DELETE CASCADE,
    event           TEXT    NOT NULL,
    body            TEXT    NOT NULL,
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    delivered_at    TEXT,
    -- The last answer: an HTTP status, or why there was none.
    last_status     INTEGER,
    last_error      TEXT,
    created_at      TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_due ON webhook_deliveries (next_attempt_at);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook ON webhook_deliveries (webhook_id, created_at);

-- When a share's end was announced. A share has no moment of expiring, only a
-- time after which it has, so a pass looks for ended shares this is NULL on.
ALTER TABLE shares ADD COLUMN end_announced_at TEXT;

CREATE INDEX IF NOT EXISTS idx_shares_end_announced ON shares (end_announced_at, expires_at);
