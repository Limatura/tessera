-- MySQL and Postgres drop a plain column without a rebuild, and so does SQLite
-- since 3.35. Nothing indexes it but the index made here.
DROP INDEX IF EXISTS idx_sessions_provider_sid;
ALTER TABLE sessions DROP COLUMN provider_sid;
