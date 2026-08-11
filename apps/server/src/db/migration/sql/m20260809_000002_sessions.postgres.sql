-- Postgres mirror of m20260809_000002_sessions.sqlite.sql.
CREATE TABLE IF NOT EXISTS sessions (
    id           TEXT        NOT NULL PRIMARY KEY,
    user_id      TEXT        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    secret_hash  TEXT        NOT NULL UNIQUE,
    created_at   TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    expires_at   TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions (user_id);

CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions (expires_at);
