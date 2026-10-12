-- Which segment a part belongs to. See the SQLite file for the problem.
--
-- `DROP PRIMARY KEY` and `ADD PRIMARY KEY` must be one statement here: the
-- foreign key on `upload_id` needs an index, and between dropping the old key
-- and adding the new one there would be none. `InnoDB` refuses the drop
-- instead of leaving the key unindexed.
ALTER TABLE upload_parts ADD COLUMN seq BIGINT NOT NULL DEFAULT 0;

ALTER TABLE upload_parts DROP PRIMARY KEY, ADD PRIMARY KEY (upload_id, seq, part_number);
