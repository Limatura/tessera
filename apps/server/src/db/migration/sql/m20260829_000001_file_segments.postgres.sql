-- Where a file's bytes are, as a list instead of a column.
--
-- # Why a list
--
-- `file_shares` had `storage` and `object_key`: one location, implicitly "the
-- store in the configuration". That had two problems.
--
-- Changing `storage.url` left every existing file pointing at a store that no
-- longer exists, with no record of where the bytes went: the share appears
-- present but fails to deliver. Recording the backend per file makes changing
-- one reversible.
--
-- And a file that fits in no single backend has nowhere to go. That is not
-- this change, but the shape it needs is: a list of `(backend, key, offset,
-- length)`. Every row has exactly one segment today, so reads cost the same,
-- but moving to a list later would mean migrating live rows; doing it now
-- costs one migration that never needs repeating.
--
-- # Why the backend is a name and not a foreign key
--
-- The set of backends is configuration, not data: it comes from the process
-- environment, and a row naming one that is no longer configured must stay
-- readable enough to report that. A foreign key would turn removing a backend
-- from the configuration into a database error at start-up, instead of a
-- message an operator can act on.
CREATE TABLE IF NOT EXISTS file_segments (
    file_id       TEXT      NOT NULL REFERENCES file_shares (id) ON DELETE CASCADE,
    -- Position in the file, from zero. One row today, so always 0.
    seq           BIGINT    NOT NULL DEFAULT 0,
    -- Which configured backend holds this segment.
    backend       TEXT      NOT NULL,
    object_key    TEXT      NOT NULL,
    -- Where this segment starts within the file, and its length.
    offset_bytes  BIGINT    NOT NULL DEFAULT 0,
    length_bytes  BIGINT    NOT NULL,
    created_at    TIMESTAMPTZ     NOT NULL,
    -- States the invariant instead of inventing an id for it: one segment per
    -- position, and no way to write two. It is also the index the reader needs,
    -- since every read is "this file, in order".
    PRIMARY KEY (file_id, seq)
);


-- Every existing file becomes exactly one segment. `gone` rows have no key
-- (their bytes were removed when the share expired), so they get no segment,
-- and a file with no segments is how "the content is no longer here" looks.
--
-- `DEFAULT_BACKEND` is the name the configuration seeds for `storage.url`, so
-- rows written before this change keep pointing at the same store.
INSERT INTO file_segments (file_id, seq, backend, object_key, offset_bytes, length_bytes, created_at)
SELECT f.id, 0, 'default', f.object_key, 0, f.size_bytes, f.created_at
  FROM file_shares f
 WHERE f.storage = 'object' AND f.object_key IS NOT NULL;
