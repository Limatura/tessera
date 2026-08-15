-- One row per file in a share.
--
-- # Why a table rather than more columns
--
-- `shares` carries `filename`, `content_type`, `size_bytes`, `storage` and
-- `object_key`, and every one of them is meaningless for a text share. That was
-- tolerable while a share held exactly one thing. It stops being tolerable the
-- moment a share can hold several, because "several" is not something more
-- columns can express — it is a second table or it is nothing.
--
-- So this is what unblocks multi-file upload, and the tidier `shares` row is a
-- consequence rather than the reason.
--
-- # What this migration deliberately does not do
--
-- It moves columns, not bytes. A file that was stored inline stays inline and
-- keeps the same `storage` value; one in the object store keeps the same key.
-- Making every file an object regardless of size is worth doing and is its own
-- migration, because that one reads and writes an object store and can fail
-- half way — a thing worth being able to diagnose on its own.
--
-- Text is untouched here for the same reason. `text_shares` in the agreed design
-- holds its content directly, which assumes text never lives in the object
-- store; today it does once it passes the inline threshold. Splitting text out
-- before that is fixed would mean carrying a "the content is actually elsewhere"
-- column on the new table — exactly the kind of column this whole change exists
-- to remove.
--
-- # Why `position`
--
-- A share of five files has an order the uploader chose, and a list that comes
-- back in whatever the database felt like is a list somebody has to re-sort by
-- filename to make sense of. One integer now is cheaper than explaining later
-- why the cover image is third.
CREATE TABLE IF NOT EXISTS file_shares (
    id            TEXT    NOT NULL PRIMARY KEY,
    share_id      TEXT    NOT NULL REFERENCES shares (id) ON DELETE CASCADE,
    position      INTEGER NOT NULL DEFAULT 0,
    filename      TEXT,
    -- Recorded, never echoed. The uploader chooses this value, and serving it
    -- back is how a text/html upload becomes stored XSS on our own origin.
    content_type  TEXT,
    size_bytes    INTEGER NOT NULL,
    -- Where the bytes live, carried over as they were. `object_key` is stored
    -- rather than derived so the layout can change without moving a byte.
    storage       TEXT    NOT NULL,
    object_key    TEXT,
    created_at    TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_file_shares_share ON file_shares (share_id, position);

-- Every existing file share becomes one row. Text shares get none: their content
-- stays in `share_contents` until the migration that gives them their own table.
--
-- The identifier is derived from the share's rather than generated, because a
-- migration has to produce the same result whenever it runs — and because a
-- share that had one file will keep pointing at the same row if this ever has to
-- be reasoned about after the fact.
INSERT INTO file_shares (id, share_id, position, filename, content_type, size_bytes, storage, object_key, created_at)
SELECT id, id, 0, filename, content_type, size_bytes, storage, object_key, created_at
  FROM shares
 WHERE kind = 'file';
