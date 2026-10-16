-- Postgres mirror of m20260809_000001_users.sqlite.sql.
--
-- `TIMESTAMPTZ` instead of the SQLite integer: Postgres has a real instant
-- type, and storing epoch seconds would throw away the reason to use it.
-- `BOOLEAN` likewise, instead of an integer flag.
CREATE TABLE IF NOT EXISTS users (
    id            TEXT        NOT NULL PRIMARY KEY,
    username      TEXT        NOT NULL,
    email         TEXT        NOT NULL,
    password_hash TEXT        NOT NULL,
    is_admin      BOOLEAN     NOT NULL DEFAULT FALSE,
    must_change_password BOOLEAN NOT NULL DEFAULT FALSE,
    email_verified_at    TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL,
    updated_at    TIMESTAMPTZ NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username ON users (username);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users (email);
