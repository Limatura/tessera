-- MySQL/MariaDB mirror of m20260813_000002_pending_registrations.sqlite.sql.
--
-- Column types follow `users` for the same reasons: VARCHAR(191) for anything
-- with a UNIQUE index, CHAR(26) for a ULID, and no `IF NOT EXISTS` on indexes
-- because MySQL lacks the clause and the migrator never runs a migration twice.
CREATE TABLE IF NOT EXISTS pending_registrations (
    id            CHAR(26)     NOT NULL PRIMARY KEY,
    username      VARCHAR(191) NOT NULL,
    email         VARCHAR(191) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    secret_hash   CHAR(64)     NOT NULL,
    created_at    TIMESTAMP(6) NOT NULL,
    expires_at    TIMESTAMP(6) NOT NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

-- One live link per address: registering again replaces the first instead of
-- leaving two working links in one mailbox.
CREATE UNIQUE INDEX idx_pending_registrations_email ON pending_registrations (email);

CREATE UNIQUE INDEX idx_pending_registrations_secret ON pending_registrations (secret_hash);

-- No index on `username`, by design: reserving a name before anyone confirms
-- it would make "register and never confirm" a free way to lock up every good
-- name. The name is checked for real when the link is followed.
CREATE INDEX idx_pending_registrations_expires_at ON pending_registrations (expires_at);
