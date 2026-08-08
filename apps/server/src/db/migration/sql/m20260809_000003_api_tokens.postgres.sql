-- Postgres mirror of m20260809_000003_api_tokens.sqlite.sql.
CREATE TABLE IF NOT EXISTS api_tokens (
    id             TEXT        NOT NULL PRIMARY KEY,
    user_id        TEXT        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name           TEXT        NOT NULL,
    secret_hash    TEXT        NOT NULL UNIQUE,
    display_prefix TEXT        NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL,
    last_used_at   TIMESTAMPTZ,
    expires_at     TIMESTAMPTZ,
    revoked_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens (user_id);
