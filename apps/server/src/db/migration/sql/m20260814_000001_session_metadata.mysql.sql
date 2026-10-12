-- MySQL/MariaDB mirror of m20260814_000001_session_metadata.sqlite.sql.
--
-- Bounded instead of TEXT, since both values have a known maximum: 45
-- characters holds the longest IPv6 address, and the repository truncates a
-- user agent to 512 before it gets here. Neither column is indexed, so the
-- widths are about row size, not key length.
ALTER TABLE sessions ADD COLUMN created_ip VARCHAR(45) NULL;

ALTER TABLE sessions ADD COLUMN created_user_agent VARCHAR(512) NULL;
