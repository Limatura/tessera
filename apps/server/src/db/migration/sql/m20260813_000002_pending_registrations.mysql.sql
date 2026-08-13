-- MySQL/MariaDB mirror of m20260813_000002_pending_registrations.sqlite.sql.
--
-- The column types follow `users` for the same reasons given there: VARCHAR(191)
-- for anything carrying a UNIQUE index, CHAR(26) for a ULID, and no
-- `IF NOT EXISTS` on indexes because MySQL has no such clause and the migrator
-- already refuses to run a migration twice.
CREATE TABLE IF NOT EXISTS pending_registrations (
    id            CHAR(26)     NOT NULL PRIMARY KEY,
    username      VARCHAR(191) NOT NULL,
    email         VARCHAR(191) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    secret_hash   CHAR(64)     NOT NULL,
    created_at    TIMESTAMP(6) NOT NULL,
    expires_at    TIMESTAMP(6) NOT NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- One live link per address: registering again replaces the first rather than
-- leaving two working links in one mailbox.
CREATE UNIQUE INDEX idx_pending_registrations_email ON pending_registrations (email);

CREATE UNIQUE INDEX idx_pending_registrations_secret ON pending_registrations (secret_hash);

-- Deliberately no index on `username`: reserving a name while nobody has
-- confirmed it would make "register and never confirm" a free way to lock every
-- good name away. The name is checked for real when the link is followed.
CREATE INDEX idx_pending_registrations_expires_at ON pending_registrations (expires_at);
