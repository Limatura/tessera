-- See the SQLite rollback: only reversible while `seq` is zero everywhere.
ALTER TABLE upload_parts DROP CONSTRAINT upload_parts_pkey;

ALTER TABLE upload_parts ADD PRIMARY KEY (upload_id, part_number);

ALTER TABLE upload_parts DROP COLUMN seq;
