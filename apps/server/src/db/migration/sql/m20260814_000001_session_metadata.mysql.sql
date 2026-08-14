-- MySQL/MariaDB mirror of m20260814_000001_session_metadata.sqlite.sql.
--
-- Bounded rather than TEXT, because both values are as long as they will ever
-- be: 45 characters holds the longest IPv6 address, and the repository truncates
-- a user agent to 512 before it reaches here. Neither column is indexed, so the
-- widths are about the row, not about a key length.
ALTER TABLE sessions ADD COLUMN created_ip VARCHAR(45) NULL;

ALTER TABLE sessions ADD COLUMN created_user_agent VARCHAR(512) NULL;
