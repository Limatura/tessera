-- Postgres mirror of m20260815_000001_invites.sqlite.sql.
CREATE TABLE IF NOT EXISTS invites (
    id          TEXT        NOT NULL PRIMARY KEY,
    code_hash   TEXT        NOT NULL UNIQUE,
    note        TEXT,
    max_uses    BIGINT      NOT NULL,
    used        BIGINT      NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_invites_expires_at ON invites (expires_at);

ALTER TABLE pending_registrations ADD COLUMN IF NOT EXISTS invite_id TEXT;
