-- Postgres mirror of m20260808_000001_initial.sqlite.sql.
-- `TIMESTAMPTZ` instead of the SQLite integer: Postgres has a real timestamp
-- type, and storing epoch seconds would throw away the reason to use it.
CREATE TABLE IF NOT EXISTS schema_metadata (
    key        TEXT        NOT NULL PRIMARY KEY,
    value      TEXT        NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
