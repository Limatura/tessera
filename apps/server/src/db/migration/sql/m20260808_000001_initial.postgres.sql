-- Postgres mirror of m20260808_000001_initial.sqlite.sql.
-- `TIMESTAMPTZ` rather than the SQLite integer: Postgres has a real timestamp
-- type and storing epoch seconds in it would throw away the reason to use it.
CREATE TABLE IF NOT EXISTS schema_metadata (
    key        TEXT        NOT NULL PRIMARY KEY,
    value      TEXT        NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
