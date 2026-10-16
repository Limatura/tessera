-- See the SQLite rollback. One statement for the key swap, for the same reason
-- the migration needed it: the foreign key on `upload_id` must never be left
-- without an index.
ALTER TABLE upload_parts DROP PRIMARY KEY, ADD PRIMARY KEY (upload_id, part_number);

ALTER TABLE upload_parts DROP COLUMN seq;
