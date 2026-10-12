-- Counts how many times a share's content was copied.
--
-- # Why a column and not a table
--
-- Both counters are one integer per share that nothing joins to, and they are
-- read alongside `use_count`, which is already a column here. A table would
-- allow events instead of totals, but events mean recording when and from
-- where, which is a different feature with its own privacy decision.
--
-- # Why `copy` and `view` are counted here and `download` is not
--
-- `use_count` is a retrieval limit, so the server must count it: a client that
-- reported its own count could decide whether the limit applied. These two
-- report actions with no side effect, which only a client can observe.

ALTER TABLE shares ADD COLUMN copy_count INTEGER NOT NULL DEFAULT 0;
