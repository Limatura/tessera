-- Postgres mirror of m20260813_000001_password_reset_tokens.sqlite.sql.
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id          TEXT        NOT NULL PRIMARY KEY,
    user_id     TEXT        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    secret_hash TEXT        NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_user ON password_reset_tokens (user_id);
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_expires_at ON password_reset_tokens (expires_at);
