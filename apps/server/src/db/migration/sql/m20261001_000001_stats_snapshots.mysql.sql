-- MySQL/MariaDB mirror of m20261001_000001_stats_snapshots.sqlite.sql.
CREATE TABLE IF NOT EXISTS stats_snapshots (
    day          CHAR(10)    NOT NULL PRIMARY KEY,
    stored_bytes BIGINT      NOT NULL,
    shares       BIGINT      NOT NULL,
    live_shares  BIGINT      NOT NULL,
    bytes_served BIGINT      NOT NULL,
    recorded_at  DATETIME(6) NOT NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
