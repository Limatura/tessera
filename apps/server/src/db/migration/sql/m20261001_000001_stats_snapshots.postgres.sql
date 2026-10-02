-- Postgres mirror of m20261001_000001_stats_snapshots.sqlite.sql.
CREATE TABLE IF NOT EXISTS stats_snapshots (
    day          TEXT        NOT NULL PRIMARY KEY,
    stored_bytes BIGINT      NOT NULL,
    shares       BIGINT      NOT NULL,
    live_shares  BIGINT      NOT NULL,
    bytes_served BIGINT      NOT NULL,
    recorded_at  TIMESTAMPTZ NOT NULL
);
