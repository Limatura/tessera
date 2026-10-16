-- Postgres mirror of m20261013_000001_mfa.sqlite.sql.
CREATE TABLE IF NOT EXISTS totp_authenticators (
    user_id        TEXT        NOT NULL PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    secret         TEXT        NOT NULL,
    confirmed_at   TIMESTAMPTZ,
    last_used_step BIGINT,
    created_at     TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS recovery_codes (
    id         TEXT        NOT NULL PRIMARY KEY,
    user_id    TEXT        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_hash  TEXT        NOT NULL,
    created_at TIMESTAMPTZ NOT NULL,
    used_at    TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_recovery_codes_user ON recovery_codes (user_id);

CREATE TABLE IF NOT EXISTS mfa_challenges (
    secret_hash TEXT        NOT NULL PRIMARY KEY,
    user_id     TEXT        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mfa_challenges_expires_at ON mfa_challenges (expires_at);
