-- Postgres mirror of m20260816_000001_file_shares.sqlite.sql.
CREATE TABLE IF NOT EXISTS file_shares (
    id            TEXT        NOT NULL PRIMARY KEY,
    share_id      TEXT        NOT NULL REFERENCES shares (id) ON DELETE CASCADE,
    position      BIGINT      NOT NULL DEFAULT 0,
    filename      TEXT,
    content_type  TEXT,
    size_bytes    BIGINT      NOT NULL,
    storage       TEXT        NOT NULL,
    object_key    TEXT,
    created_at    TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_file_shares_share ON file_shares (share_id, position);

INSERT INTO file_shares (id, share_id, position, filename, content_type, size_bytes, storage, object_key, created_at)
SELECT id, id, 0, filename, content_type, size_bytes, storage, object_key, created_at
  FROM shares
 WHERE kind = 'file';
