-- Counts how many times a share's content was copied.
--
-- # Why a second column and not a second table
--
-- Both of these are one integer per share that nothing else joins to, and the
-- question they answer is asked in the same breath as `use_count` — which is
-- already a column here. A table would buy the ability to hold events rather
-- than totals, and holding events means holding when and from where, which is
-- a different feature with a different privacy decision behind it.
--
-- # Why `copy` and `view` are counted and `download` is not
--
-- `use_count` is a retrieval limit, so the server has to be the one counting
-- it — a client that could report its own would be a client that could decide
-- whether the limit applied. These two are reports of something with no side
-- effect, so a client is the only thing that can see them happen at all.

ALTER TABLE shares ADD COLUMN copy_count INTEGER NOT NULL DEFAULT 0;
