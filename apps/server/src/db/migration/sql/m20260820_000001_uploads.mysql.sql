-- Resumable uploads: a file that arrives across several requests.
--
-- # Why a session and not a partial `file_shares` row
--
-- A share is created by its first file and grown by the rest, and nothing has
-- to remember to finish. An upload that has not finished is not a file yet, so
-- putting it in `file_shares` would mean every reader learning to skip rows
-- that are not really there — including the archive writer, the size
-- prediction and the quota.
--
-- So an unfinished upload lives here, and moves to `file_shares` in one insert
-- when its last byte lands.
--
-- # Why the key is decided once
--
-- `object_key` is written at creation and never recomputed. FileCodeBox #362 is
-- what happens otherwise: their chunk path was derived from the current date,
-- so an upload resumed the next day wrote to a different directory and the
-- merge could not find yesterday's chunks. Our `object_key` is
-- `{yyyy}/{mm}/{id}` — the same shape, and the same bug if it were recomputed
-- per request.
--
-- # Why `share_id` is nullable
--
-- Null means "this becomes a new share when it finishes"; set means "append to
-- that one". Deciding at creation rather than at completion keeps the finish
-- step from needing anything the client has to remember.

CREATE TABLE uploads (
    id             VARCHAR(191) NOT NULL PRIMARY KEY,
    -- The share to append to, or NULL to create one on completion.
    share_id       VARCHAR(191),
    -- Whoever may continue it: the account, or NULL for an anonymous upload
    -- holding the token below.
    user_id        VARCHAR(191),
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
    created_at     DATETIME(6) NOT NULL,
    updated_at     DATETIME(6) NOT NULL,
    expires_at     DATETIME(6) NOT NULL,
    CONSTRAINT fk_uploads_share FOREIGN KEY (share_id) REFERENCES shares (id) ON DELETE CASCADE,
    CONSTRAINT fk_uploads_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- The sweep reads this, and it is the only query that is not by primary key.
CREATE INDEX idx_uploads_expires ON uploads (expires_at);

-- What `complete_multipart` needs handed back, in order.
--
-- Kept as rows rather than rebuilt by listing the store: `ListParts` is another
-- round trip that not every backend implements the same way, and the numbers
-- are ours to remember.
CREATE TABLE upload_parts (
    upload_id   VARCHAR(191) NOT NULL,
    part_number INT NOT NULL,
    -- The store's identifier for the part, its ETag on S3.
    content_id  TEXT NOT NULL,
    PRIMARY KEY (upload_id, part_number),
    CONSTRAINT fk_upload_parts_upload FOREIGN KEY (upload_id) REFERENCES uploads (id) ON DELETE CASCADE
);
