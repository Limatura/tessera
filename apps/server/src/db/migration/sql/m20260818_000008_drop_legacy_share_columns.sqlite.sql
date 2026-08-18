-- Removes the content columns from `shares`, and the table they shared.
--
-- # What was here
--
-- `filename`, `content_type`, `language`, `size_bytes`, `storage` and
-- `object_key` described what a share held. They worked while a share held
-- exactly one thing. They stop working the moment it can hold several, which is
-- what #42 is: `filename` has no honest value for a share with three files, and
-- keeping it would mean either a lie or a second copy that drifts.
--
-- `file_shares` and `text_shares` have owned this since #103 and #109, and the
-- readers moved across in #111. What is left here is the copy nobody reads.
--
-- # Why now and not later
--
-- Writing #42 against these columns would mean writing code to maintain
-- something it cannot maintain honestly. Removing them first makes the subtype
-- tables the only answer, and #42 becomes "add more rows".
--
-- # `share_contents`
--
-- The old home for inline bytes. `text_shares` holds text now, and files always
-- become objects — so nothing writes here and nothing reads it.
--
-- # Why `DROP COLUMN` and not a rebuild
--
-- SQLite has supported it since 3.35 and refuses only for a column an index or
-- constraint names. None of these six is either; checked against the real
-- schema rather than assumed.

ALTER TABLE shares DROP COLUMN filename;
ALTER TABLE shares DROP COLUMN content_type;
ALTER TABLE shares DROP COLUMN language;
ALTER TABLE shares DROP COLUMN size_bytes;
ALTER TABLE shares DROP COLUMN storage;
ALTER TABLE shares DROP COLUMN object_key;

DROP TABLE share_contents;
