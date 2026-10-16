-- Postgres mirror of m20261013_000002_passkeys.sqlite.sql.
CREATE TABLE IF NOT EXISTS passkeys (
    id              TEXT        NOT NULL PRIMARY KEY,
    user_id         TEXT        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    credential_hash TEXT        NOT NULL UNIQUE,
    credential_id   TEXT        NOT NULL,
    public_key      TEXT        NOT NULL,
    sign_count      BIGINT      NOT NULL,
    name            TEXT        NOT NULL,
    backup_eligible BOOLEAN     NOT NULL,
    backed_up       BOOLEAN     NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL,
    last_used_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_passkeys_user ON passkeys (user_id);

CREATE TABLE IF NOT EXISTS webauthn_challenges (
    secret_hash TEXT        NOT NULL PRIMARY KEY,
    purpose     TEXT        NOT NULL,
    user_id     TEXT REFERENCES users (id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webauthn_challenges_expires_at ON webauthn_challenges (expires_at);
