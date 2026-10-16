-- Removes the content columns from `shares`, and the table they shared.
--
-- # What was here
--
-- `filename`, `content_type`, `language`, `size_bytes`, `storage` and
-- `object_key` described what a share held. They worked while a share held
-- exactly one thing, and stop working once it can hold several (multi-file
-- shares): `filename` has no correct value for a three-file share, and keeping
-- it would mean either a false value or a second copy that drifts.
--
-- `file_shares` and `text_shares` already own this data, and every reader has
-- moved over to them. What remains here is an unread copy.
--
-- # Why now
--
-- Building multi-file shares on these columns would mean maintaining something
-- that cannot be kept correct. Removing them first makes the subtype tables
-- the only source, and multi-file shares become "add more rows".
--
-- # `share_contents`
--
-- The old home for inline bytes. `text_shares` holds text now, and files
-- always become objects, so nothing writes or reads it.
--
ALTER TABLE shares
    DROP COLUMN filename,
    DROP COLUMN content_type,
    DROP COLUMN language,
    DROP COLUMN size_bytes,
    DROP COLUMN storage,
    DROP COLUMN object_key;

DROP TABLE share_contents;
