-- MySQL/MariaDB mirror of m20260817_000001_text_shares.sqlite.sql.
--
-- LONGBLOB rather than BLOB: the latter stops at 64 KiB, which is exactly the
-- size a text share is allowed to pass.
CREATE TABLE IF NOT EXISTS text_shares (
    share_id CHAR(26)     NOT NULL PRIMARY KEY,
    language VARCHAR(64),
    content  LONGBLOB     NOT NULL,
    CONSTRAINT fk_text_shares_share FOREIGN KEY (share_id) REFERENCES shares (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

INSERT INTO text_shares (share_id, language, content)
SELECT s.id, s.language, c.content
  FROM shares s
  JOIN share_contents c ON c.share_id = s.id
 WHERE s.kind = 'text';
