-- `TIMESTAMP(6)` and never a bare `TIMESTAMP`: MySQL's default precision is
-- whole seconds, so a timestamp written from Rust would silently lose its
-- fractional part and no longer equal what was stored. Six digits matches
-- Postgres's `TIMESTAMPTZ`, which makes all three backends agree.
--
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
    updated_at TIMESTAMP(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6)
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
