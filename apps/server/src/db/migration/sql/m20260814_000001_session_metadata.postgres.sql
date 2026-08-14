-- Postgres mirror of m20260814_000001_session_metadata.sqlite.sql.
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS created_ip TEXT;

ALTER TABLE sessions ADD COLUMN IF NOT EXISTS created_user_agent TEXT;
