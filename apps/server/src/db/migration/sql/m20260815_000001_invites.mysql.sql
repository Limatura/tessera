-- MySQL/MariaDB mirror of m20260815_000001_invites.sqlite.sql.
--
-- Fixed widths for the two indexed columns, as elsewhere: a ULID is 26
-- characters and a hex SHA-256 digest is 64.
CREATE TABLE IF NOT EXISTS invites (
    id          CHAR(26)     NOT NULL PRIMARY KEY,
    code_hash   CHAR(64)     NOT NULL UNIQUE,
    note        VARCHAR(191),
    max_uses    BIGINT       NOT NULL,
    used        BIGINT       NOT NULL DEFAULT 0,
    created_at  TIMESTAMP(6) NOT NULL,
    expires_at  TIMESTAMP(6) NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_invites_expires_at ON invites (expires_at);

ALTER TABLE pending_registrations ADD COLUMN invite_id CHAR(26) NULL;
