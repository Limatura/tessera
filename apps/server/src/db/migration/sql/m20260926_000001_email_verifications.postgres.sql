-- Postgres mirror of m20260926_000001_email_verifications.sqlite.sql.
CREATE TABLE IF NOT EXISTS email_verifications (
    id          TEXT        NOT NULL PRIMARY KEY,
    user_id     TEXT        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    email       TEXT        NOT NULL,
    secret_hash TEXT        NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_email_verifications_user ON email_verifications (user_id);
CREATE INDEX IF NOT EXISTS idx_email_verifications_expires_at ON email_verifications (expires_at);
