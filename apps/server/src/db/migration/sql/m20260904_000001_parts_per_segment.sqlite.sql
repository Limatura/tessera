-- Which segment a part belongs to.
--
-- # What was wrong
--
-- `upload_parts` was keyed on `(upload_id, part_number)`, and a part number is
-- the store's own, restarting at one for every multipart upload. One session,
-- one upload, so that held.
--
-- A session spread over two backends is two multipart uploads. Both hand back a
-- part one, a part two, a part three — and the second segment's rows collide
-- with the first's on the primary key. Whichever way the database resolves that,
-- the result is a `complete_multipart` handed somebody else's `ETag`s: an object
-- that either fails to assemble or assembles wrong.
--
-- Nothing splits yet, so nothing has hit this. It has to go in before anything
-- does, because by then the rows exist and the fix is a data migration rather
-- than a column.
--
-- # Why the whole table is rebuilt, and in this order
--
-- SQLite cannot add a column to a primary key, so the table is built again and
-- the rows copied across. Everything else about it is unchanged, and `seq` is
-- zero, which is the segment every existing row belongs to.
--
-- The old table is renamed *out of the way* first rather than dropped last.
-- Dropping `upload_parts` and then renaming something to `upload_parts` in the
-- same transaction is refused — "there is already another table or index with
-- this name", on a name `sqlite_master` no longer has. Measured: the statements
-- run clean one at a time and fail as a migration. Never reusing a name freed
-- in the same transaction avoids the question entirely, and is the order
-- SQLite's own documentation gives for rebuilding a table.
ALTER TABLE upload_parts RENAME TO upload_parts_old;

CREATE TABLE upload_parts (
    upload_id   TEXT NOT NULL REFERENCES uploads (id) ON DELETE CASCADE,
    -- Position of the segment in the file. Zero until a file is split.
    seq         INTEGER NOT NULL DEFAULT 0,
    part_number INTEGER NOT NULL,
    -- The store's identifier for the part, its ETag on S3.
    content_id  TEXT NOT NULL,
    PRIMARY KEY (upload_id, seq, part_number)
);

INSERT INTO upload_parts (upload_id, seq, part_number, content_id)
SELECT upload_id, 0, part_number, content_id FROM upload_parts_old;

DROP TABLE upload_parts_old;
