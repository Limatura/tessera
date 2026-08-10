-- Postgres mirror of m20260810_000002_share_tickets.sqlite.sql.
CREATE TABLE IF NOT EXISTS share_tickets (
    id          TEXT        NOT NULL PRIMARY KEY,
    share_id    TEXT        NOT NULL REFERENCES shares (id) ON DELETE CASCADE,
    secret_hash TEXT        NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_share_tickets_share ON share_tickets (share_id);
CREATE INDEX IF NOT EXISTS idx_share_tickets_expires_at ON share_tickets (expires_at);
