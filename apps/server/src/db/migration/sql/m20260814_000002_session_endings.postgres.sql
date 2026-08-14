-- Postgres mirror of m20260814_000002_session_endings.sqlite.sql.
ALTER TABLE sessions ADD COLUMN IF NOT EXISTS ended_reason TEXT;
