-- One row per file in a share.
--
-- # Why a table instead of more columns
--
-- `shares` carries `filename`, `content_type`, `size_bytes`, `storage` and
-- `object_key`, all meaningless for a text share. That was tolerable while a
-- share held exactly one thing, but not once it can hold several: "several"
-- cannot be expressed with more columns, only with a second table.
--
-- So this is what unblocks multi-file upload; the tidier `shares` row is a
-- side effect, not the reason.
--
-- # What this migration does not do
--
-- It moves columns, not bytes. A file stored inline stays inline with the same
-- `storage` value; one in the object store keeps its key. Making every file an
-- object regardless of size is worth doing, but is its own migration: it reads
-- and writes an object store and can fail halfway, which should be diagnosable
-- on its own.
--
-- Text is untouched here for the same reason. `text_shares` in the agreed
-- design holds its content directly, assuming text never lives in the object
-- store; today it does once past the inline threshold. Splitting text out
-- before that is fixed would need a "content is actually elsewhere" column on
-- the new table, the kind of column this change exists to remove.
--
-- # Why `position`
--
-- A five-file share has an order the uploader chose, and a list in arbitrary
-- database order has to be re-sorted by filename to make sense. One integer
-- now is cheaper than explaining later why the cover image is third.
CREATE TABLE IF NOT EXISTS file_shares (
    id            TEXT    NOT NULL PRIMARY KEY,
    share_id      TEXT    NOT NULL REFERENCES shares (id) ON DELETE CASCADE,
    position      INTEGER NOT NULL DEFAULT 0,
    filename      TEXT,
    -- Recorded, never echoed. The uploader chooses this value, and serving it
    -- back is how a text/html upload becomes stored XSS on our own origin.
    content_type  TEXT,
    size_bytes    INTEGER NOT NULL,
    -- Where the bytes live, carried over unchanged. `object_key` is stored, not
    -- derived, so the layout can change without moving a byte.
    storage       TEXT    NOT NULL,
    object_key    TEXT,
    created_at    TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_file_shares_share ON file_shares (share_id, position);

-- Every existing file share becomes one row. Text shares get none: their
-- content stays in `share_contents` until the migration that gives them their
-- own table.
--
-- The id is derived from the share's instead of generated, because a
-- migration must produce the same result whenever it runs, and so a one-file
-- share maps to a predictable row if this ever needs investigating.
INSERT INTO file_shares (id, share_id, position, filename, content_type, size_bytes, storage, object_key, created_at)
SELECT id, id, 0, filename, content_type, size_bytes, storage, object_key, created_at
  FROM shares
 WHERE kind = 'file';
