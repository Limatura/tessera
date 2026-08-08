-- MySQL/MariaDB mirror of m20260808_000001_initial.sqlite.sql.
--
-- Two differences that are not stylistic:
--   * `key` is a reserved word in MySQL, so it must be quoted.
--   * `TEXT` cannot be a primary key without a prefix length. VARCHAR(191) is
--     the largest utf8mb4 column that fits the 767-byte index limit on older
--     InnoDB builds, which homelab installs still run.
CREATE TABLE IF NOT EXISTS schema_metadata (
    `key`      VARCHAR(191) NOT NULL PRIMARY KEY,
    `value`    TEXT         NOT NULL,
    updated_at TIMESTAMP    NOT NULL DEFAULT CURRENT_TIMESTAMP
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
