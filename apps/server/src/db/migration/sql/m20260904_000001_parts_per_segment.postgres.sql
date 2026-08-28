-- Which segment a part belongs to. See the SQLite file for what was wrong.
--
-- Postgres can add the column and swap the key in place, so it does.
ALTER TABLE upload_parts ADD COLUMN seq BIGINT NOT NULL DEFAULT 0;

ALTER TABLE upload_parts DROP CONSTRAINT upload_parts_pkey;

ALTER TABLE upload_parts ADD PRIMARY KEY (upload_id, seq, part_number);
