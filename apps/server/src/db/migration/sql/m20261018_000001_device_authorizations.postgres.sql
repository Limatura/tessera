-- Postgres mirror of m20261018_000001_device_authorizations.sqlite.sql.
CREATE TABLE IF NOT EXISTS device_authorizations (
    device_code_hash TEXT        NOT NULL PRIMARY KEY,
    user_code        TEXT        NOT NULL UNIQUE,
    client_name      TEXT        NOT NULL,
    requested_from   TEXT        NOT NULL,
    user_id          TEXT REFERENCES users (id) ON DELETE CASCADE,
    permissions      TEXT,
    approved_at      TIMESTAMPTZ,
    denied_at        TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL,
    expires_at       TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_device_authorizations_expires_at ON device_authorizations (expires_at);
