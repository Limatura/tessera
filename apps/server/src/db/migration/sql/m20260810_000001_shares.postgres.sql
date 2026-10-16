-- Postgres mirror of m20260810_000001_shares.sqlite.sql.
CREATE TABLE IF NOT EXISTS shares (
    id             TEXT        NOT NULL PRIMARY KEY,
    code           TEXT        NOT NULL UNIQUE,
    user_id        TEXT        REFERENCES users (id) ON DELETE SET NULL,
    kind           TEXT        NOT NULL,

    password_hash  TEXT,
    use_limit      INTEGER,
    use_count      INTEGER     NOT NULL DEFAULT 0,
    expires_at     TIMESTAMPTZ,
    disabled_at    TIMESTAMPTZ,

    title          TEXT,
    filename       TEXT,
    content_type   TEXT,
    language       TEXT,
    size_bytes     BIGINT      NOT NULL,

    storage        TEXT        NOT NULL,
    object_key     TEXT,

    og_config      TEXT,
    management_token_hash TEXT,
    view_count     INTEGER     NOT NULL DEFAULT 0,
    last_accessed_at TIMESTAMPTZ,

    created_at     TIMESTAMPTZ NOT NULL,
    updated_at     TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_shares_user ON shares (user_id);
CREATE INDEX IF NOT EXISTS idx_shares_expires_at ON shares (expires_at);

CREATE TABLE IF NOT EXISTS share_contents (
    share_id TEXT  NOT NULL PRIMARY KEY REFERENCES shares (id) ON DELETE CASCADE,
    content  BYTEA NOT NULL
);
