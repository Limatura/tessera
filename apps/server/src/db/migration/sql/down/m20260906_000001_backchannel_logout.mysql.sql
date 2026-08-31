-- MySQL has no `DROP INDEX IF EXISTS`, and dropping the column takes its index
-- with it, so there is nothing to drop separately.
ALTER TABLE sessions DROP COLUMN provider_sid;
