-- Which segment a part belongs to. See the SQLite file for what was wrong.
--
-- `DROP PRIMARY KEY` and `ADD PRIMARY KEY` have to be one statement here: the
-- foreign key on `upload_id` needs an index, and between dropping the old key
-- and adding the new one there would not be one. `InnoDB` refuses the drop
-- rather than leaving the key unindexed.
ALTER TABLE upload_parts ADD COLUMN seq BIGINT NOT NULL DEFAULT 0;

ALTER TABLE upload_parts DROP PRIMARY KEY, ADD PRIMARY KEY (upload_id, seq, part_number);
