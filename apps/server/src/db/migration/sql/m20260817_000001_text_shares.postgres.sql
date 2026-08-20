-- Postgres mirror of m20260817_000001_text_shares.sqlite.sql.
CREATE TABLE IF NOT EXISTS text_shares (
    share_id TEXT NOT NULL PRIMARY KEY REFERENCES shares (id) ON DELETE CASCADE,
    language TEXT,
    content  BYTEA NOT NULL
);

INSERT INTO text_shares (share_id, language, content)
SELECT s.id, s.language, c.content
  FROM shares s
  JOIN share_contents c ON c.share_id = s.id
 WHERE s.kind = 'text';
