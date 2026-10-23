-- Postgres mirror of m20261023_000002_webhooks.sqlite.sql.
CREATE TABLE IF NOT EXISTS webhooks (
    id         TEXT        NOT NULL PRIMARY KEY,
    user_id    TEXT REFERENCES users (id) ON DELETE CASCADE,
    url        TEXT        NOT NULL,
    events     TEXT        NOT NULL,
    format     TEXT        NOT NULL,
    secret     TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webhooks_user ON webhooks (user_id);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id              TEXT        NOT NULL PRIMARY KEY,
    webhook_id      TEXT        NOT NULL REFERENCES webhooks (id) ON DELETE CASCADE,
    event           TEXT        NOT NULL,
    body            TEXT        NOT NULL,
    attempts        INTEGER     NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ,
    delivered_at    TIMESTAMPTZ,
    last_status     INTEGER,
    last_error      TEXT,
    created_at      TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_due ON webhook_deliveries (next_attempt_at);
CREATE INDEX IF NOT EXISTS idx_webhook_deliveries_webhook ON webhook_deliveries (webhook_id, created_at);

ALTER TABLE shares ADD COLUMN end_announced_at TIMESTAMPTZ;

CREATE INDEX IF NOT EXISTS idx_shares_end_announced ON shares (end_announced_at, expires_at);
