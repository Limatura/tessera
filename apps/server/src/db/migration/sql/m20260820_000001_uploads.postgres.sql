-- Resumable uploads: a file that arrives across several requests.
--
-- # Why a session and not a partial `file_shares` row
--
-- A share is created by its first file and grown by the rest, and nothing has
-- to be finalised. An unfinished upload is not a file yet, so putting it in
-- `file_shares` would make every reader skip rows that are not really there,
-- including the archive writer, the size prediction and the quota.
--
-- So an unfinished upload lives here, and moves to `file_shares` in one insert
-- when its last byte arrives.
--
-- # Why the key is decided once
--
-- `object_key` is written at creation and never recomputed.
-- `vastsa/FileCodeBox#362` shows why: their chunk path was derived from the
-- current date, so an upload resumed the next day wrote to a different
-- directory and the merge could not find the earlier chunks. Our `object_key`
-- is `{yyyy}/{mm}/{id}`, the same shape, with the same bug if it were
-- recomputed per request.
--
-- # Why `share_id` is nullable
--
-- Null means "becomes a new share when finished"; set means "append to that
-- share". Deciding at creation instead of completion means the finish step
-- needs nothing the client has to remember.

CREATE TABLE uploads (
    id             TEXT NOT NULL PRIMARY KEY,
    -- The share to append to, or NULL to create one on completion.
    share_id       TEXT REFERENCES shares (id) ON DELETE CASCADE,
    -- Whoever may continue it: the account, or NULL for an anonymous upload
    -- holding the token below.
    user_id        TEXT REFERENCES users (id) ON DELETE CASCADE,
    token_hash     TEXT NOT NULL,
    filename       TEXT,
    content_type   TEXT,
    -- What the client said it would send. The upload finishes when `offset`
    -- reaches it.
    declared_bytes BIGINT NOT NULL,
    -- How much has landed. The answer to a `HEAD`.
    offset_bytes   BIGINT NOT NULL DEFAULT 0,
    -- Decided once, at creation. See above.
    object_key     TEXT NOT NULL,
    -- The store's own multipart identifier, for the backends that have one.
    -- NULL on a local filesystem, which appends to a file instead.
    multipart_id   TEXT,
    created_at     TIMESTAMPTZ NOT NULL,
    updated_at     TIMESTAMPTZ NOT NULL,
    expires_at     TIMESTAMPTZ NOT NULL
);

-- The sweep reads this, and it is the only query that is not by primary key.
CREATE INDEX idx_uploads_expires ON uploads (expires_at);

-- What `complete_multipart` needs handed back, in order.
--
-- Stored as rows instead of rebuilt by listing the store: `ListParts` is
-- another round trip that backends implement inconsistently, and the numbers
-- are ours to remember.
CREATE TABLE upload_parts (
    upload_id   TEXT NOT NULL REFERENCES uploads (id) ON DELETE CASCADE,
    part_number INTEGER NOT NULL,
    -- The store's identifier for the part, its ETag on S3.
    content_id  TEXT NOT NULL,
    PRIMARY KEY (upload_id, part_number)
);
