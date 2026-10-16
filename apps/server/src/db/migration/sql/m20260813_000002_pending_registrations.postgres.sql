-- Postgres mirror of m20260813_000002_pending_registrations.sqlite.sql.
CREATE TABLE IF NOT EXISTS pending_registrations (
    id            TEXT        NOT NULL PRIMARY KEY,
    username      TEXT        NOT NULL,
    email         TEXT        NOT NULL UNIQUE,
    password_hash TEXT        NOT NULL,
    secret_hash   TEXT        NOT NULL UNIQUE,
    created_at    TIMESTAMPTZ NOT NULL,
    expires_at    TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pending_registrations_expires_at ON pending_registrations (expires_at);
