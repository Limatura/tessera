-- MySQL/MariaDB mirror of m20260816_000001_file_shares.sqlite.sql.
--
-- Widths follow the neighbouring tables: CHAR(26) for a ULID, VARCHAR(191) for
-- anything that might be indexed.
CREATE TABLE IF NOT EXISTS file_shares (
    id            CHAR(26)     NOT NULL PRIMARY KEY,
    share_id      CHAR(26)     NOT NULL,
    position      BIGINT       NOT NULL DEFAULT 0,
    filename      VARCHAR(512),
    content_type  VARCHAR(255),
    size_bytes    BIGINT       NOT NULL,
    storage       VARCHAR(16)  NOT NULL,
    object_key    VARCHAR(512),
    created_at    TIMESTAMP(6) NOT NULL,
    CONSTRAINT fk_file_shares_share FOREIGN KEY (share_id) REFERENCES shares (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_file_shares_share ON file_shares (share_id, position);

INSERT INTO file_shares (id, share_id, position, filename, content_type, size_bytes, storage, object_key, created_at)
SELECT id, id, 0, filename, content_type, size_bytes, storage, object_key, created_at
  FROM shares
 WHERE kind = 'file';
