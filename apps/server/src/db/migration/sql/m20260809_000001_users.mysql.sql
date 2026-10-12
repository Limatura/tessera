-- MySQL/MariaDB mirror of m20260809_000001_users.sqlite.sql.
--
-- Differences that are not stylistic:
--   * `TEXT` cannot be indexed without a prefix length, and both `username` and
--     `email` need a UNIQUE index, so they are VARCHAR. 191 is the largest
--     utf8mb4 column that fits the 767-byte index limit on older InnoDB builds,
--     which homelab installs still run.
--   * `id` is 26 characters (a ULID), so CHAR(26) is exact and fixed-width.
--   * MySQL has no `CREATE INDEX IF NOT EXISTS`. It is not needed: the migrator
--     records what it has applied, so no migration runs twice. The clause stays
--     on the other dialects only because it costs nothing there.
CREATE TABLE IF NOT EXISTS users (
    id            CHAR(26)     NOT NULL PRIMARY KEY,
    username      VARCHAR(191) NOT NULL,
    email         VARCHAR(191) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    is_admin      BOOLEAN      NOT NULL DEFAULT FALSE,
    must_change_password BOOLEAN NOT NULL DEFAULT FALSE,
    email_verified_at    TIMESTAMP(6) NULL DEFAULT NULL,
    created_at    TIMESTAMP(6) NOT NULL,
    updated_at    TIMESTAMP(6) NOT NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE UNIQUE INDEX idx_users_username ON users (username);

CREATE UNIQUE INDEX idx_users_email ON users (email);
