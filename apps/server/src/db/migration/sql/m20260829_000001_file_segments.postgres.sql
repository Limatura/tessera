-- Where a file's bytes are, as a list rather than a column.
--
-- # Why a list
--
-- `file_shares` said `storage` and `object_key`: one place, and that place was
-- implicitly "the store in the configuration". Two things were wrong with that.
--
-- Changing `storage.url` left every existing file pointing at a store that is
-- no longer there, with nothing recording where the bytes actually went — the
-- share reads as present and fails to deliver. Recording the backend per file
-- is what makes changing one a reversible action.
--
-- And a file that does not fit in any single backend has nowhere to go. That is
-- not this change, but the shape it needs is: a list of `(backend, key, offset,
-- length)`. Every row here has exactly one segment today, so reading is the
-- same work it was — but arriving at the list later would mean migrating live
-- rows, and arriving at it now costs one migration nobody has to repeat.
--
-- # Why the backend is a name and not a foreign key
--
-- The set of backends is configuration, not data: it comes from the process's
-- environment, and a row naming one that is no longer configured has to be
-- readable enough to say so. A foreign key would make removing a backend from
-- the configuration into a database error at start-up rather than a message an
-- operator can act on.
CREATE TABLE IF NOT EXISTS file_segments (
    file_id       TEXT      NOT NULL REFERENCES file_shares (id) ON DELETE CASCADE,
    -- Position in the file, from zero. One row today, so always 0.
    seq           BIGINT    NOT NULL DEFAULT 0,
    -- Which configured backend holds this segment.
    backend       TEXT      NOT NULL,
    object_key    TEXT      NOT NULL,
    -- Where this segment starts within the whole file, and how long it is.
    offset_bytes  BIGINT    NOT NULL DEFAULT 0,
    length_bytes  BIGINT    NOT NULL,
    created_at    TIMESTAMPTZ     NOT NULL,
    -- Says the invariant rather than inventing an identifier for it: one
    -- segment per position, and no way to write two. It is also the index the
    -- reader needs, since every read is "this file, in order".
    PRIMARY KEY (file_id, seq)
);


-- Every existing file becomes exactly one segment. `gone` rows carry no key —
-- their bytes were removed when the share died — so they get no segment, and a
-- file with no segments is what "the content is not here any more" looks like.
--
-- `DEFAULT_BACKEND` is the name the configuration seeds for `storage.url`, so
-- rows written before this change keep pointing at the same store they always
-- did.
INSERT INTO file_segments (file_id, seq, backend, object_key, offset_bytes, length_bytes, created_at)
SELECT f.id, 0, 'default', f.object_key, 0, f.size_bytes, f.created_at
  FROM file_shares f
 WHERE f.storage = 'object' AND f.object_key IS NOT NULL;
