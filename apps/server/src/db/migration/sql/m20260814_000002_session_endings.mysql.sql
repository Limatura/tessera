-- MySQL/MariaDB mirror of m20260814_000002_session_endings.sqlite.sql.
--
-- Bounded because the values are a closed set written by this service, never
-- caller input. The longest is `password_reset`.
ALTER TABLE sessions ADD COLUMN ended_reason VARCHAR(32) NULL;
