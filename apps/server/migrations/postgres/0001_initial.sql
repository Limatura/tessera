-- Postgres mirror of migrations/sqlite/0001_initial.sql. The two directories are
-- kept separate rather than sharing one dialect-neutral file: sqlx tracks
-- checksums per migration, and the dialects diverge as soon as real domain tables
-- appear (BLOB vs BYTEA, INTEGER timestamps vs TIMESTAMPTZ).
CREATE TABLE IF NOT EXISTS schema_metadata (
    key        TEXT        NOT NULL PRIMARY KEY,
    value      TEXT        NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
