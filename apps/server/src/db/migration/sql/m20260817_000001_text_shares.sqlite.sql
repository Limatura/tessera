-- Text and code, stored where text belongs.
--
-- # Why text stops sharing a table with files
--
-- `shares.language` means nothing for a file, and `file_shares` now holds
-- everything that means nothing for text. What remains is one column named for
-- one kind, and the content in a table called `share_contents` that until now
-- held file bytes too.
--
-- # Why the content column is not nullable
--
-- A text share whose content lives elsewhere is the state this change exists
-- to remove. So a row here means "the text is here", and a share with no row
-- here has not been moved yet, which a reader can tell without a flag that
-- must be kept accurate.
--
-- # Why this migration cannot finish the job
--
-- A text share past `share.inline_threshold_bytes` is in the object store
-- today, and pulling it back means reading an object, which SQL cannot do and
-- a migration should not try. The housekeeping sweep moves those rows a few at
-- a time, and until then they keep working from where they are.
--
-- The alternative was an upgrade step operators must remember to run, whose
-- failure mode is an instance quietly serving nothing.
CREATE TABLE IF NOT EXISTS text_shares (
    share_id TEXT NOT NULL PRIMARY KEY REFERENCES shares (id) ON DELETE CASCADE,
    language TEXT,
    content  BLOB NOT NULL
);

INSERT INTO text_shares (share_id, language, content)
SELECT s.id, s.language, c.content
  FROM shares s
  JOIN share_contents c ON c.share_id = s.id
 WHERE s.kind = 'text';
