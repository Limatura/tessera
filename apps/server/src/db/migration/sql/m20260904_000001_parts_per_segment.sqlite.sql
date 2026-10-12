-- Which segment a part belongs to.
--
-- # The problem
--
-- `upload_parts` was keyed on `(upload_id, part_number)`, and part numbers
-- come from the store, restarting at one for each multipart upload. With one
-- multipart upload per session, that held.
--
-- A session spread over two backends is two multipart uploads. Both return a
-- part one, two, three, and the second segment's rows collide with the first's
-- on the primary key. However the database resolves that, `complete_multipart`
-- gets another segment's `ETag`s: the object either fails to assemble or
-- assembles wrong.
--
-- Nothing splits yet, so nothing has hit this. It must land first, because
-- once such rows exist the fix is a data migration instead of a column.
--
-- # Why the table is rebuilt, in this order
--
-- SQLite cannot add a column to a primary key, so the table is rebuilt and the
-- rows copied over. Everything else is unchanged, and `seq` is zero, the
-- segment every existing row belongs to.
--
-- The old table is renamed *out of the way* first instead of dropped last.
-- Dropping `upload_parts` and then renaming something to `upload_parts` in the
-- same transaction is refused ("there is already another table or index with
-- this name", for a name `sqlite_master` no longer has). Tested: the
-- statements succeed one at a time but fail as a migration. Never reusing a
-- name freed in the same transaction avoids the issue, and is the order
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
