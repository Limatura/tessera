-- Where an in-progress upload is writing its bytes, as a list instead of
-- three columns.
--
-- # What the columns could not express
--
-- `uploads` carried `backend`, `object_key` and `multipart_id`: one
-- destination, fixed when the session opened. A file that fits in no single
-- backend must be written to several in turn, and three singular columns
-- cannot say "the first is finished, the second not opened yet". This table
-- can.
--
-- Every session has exactly one row here today, so nothing behaves
-- differently, but moving to a list later would mean migrating sessions
-- mid-flight; doing it now costs one migration that never needs repeating.
--
-- # Why `state` and not a pair of nullable columns
--
-- "Has a multipart id" and "has been written to" are different questions, and
-- neither is "is this segment finished". A local filesystem has no multipart
-- id, so inferring state from that column would make every local segment look
-- unopened forever. Three named states say it once:
--
--   planned   space is reserved and nothing has been written
--   open      the destination is committed and bytes may be written
--   complete  every byte of this segment is in the store
--
-- `planned` has no writer yet, intentionally: it is the reservation, and
-- nothing reserves until the planner spreads a file over several backends.
-- Two large uploads arriving together must not both be planned onto the same
-- backend's last free gigabyte, and a row existing before the first byte is
-- what will prevent that. Every session today commits its destination on
-- opening, so every row is `open`.
--
-- # Why the sweep cares
--
-- A `complete` segment holds real space in a real bucket while its share does
-- not exist yet. Previously nothing could see that: the space was used, the
-- planner could not count it, and a free-tier bucket filled up with no query
-- able to explain why. Cleanup now aborts every `open` row and deletes the
-- object behind every `complete` one: the same contract as before, per
-- segment.
CREATE TABLE IF NOT EXISTS upload_segments (
    -- `VARCHAR(191)`, not `TEXT`: this column is in the primary key, and
    -- `InnoDB` cannot index `TEXT` without a prefix length.
    upload_id     VARCHAR(191) NOT NULL,
    -- Position in the file, from zero. One row today, so always 0.
    seq           BIGINT    NOT NULL DEFAULT 0,
    -- Which configured backend this piece goes to.
    backend       VARCHAR(191) NOT NULL,
    object_key    TEXT      NOT NULL,
    -- Where this piece starts within the file, and how long it will be. How much
    -- has actually arrived is not here: `uploads.offset_bytes` already records it
    -- for the whole file, and a second copy would be one more thing to keep in
    -- sync.
    offset_bytes  BIGINT    NOT NULL DEFAULT 0,
    length_bytes  BIGINT    NOT NULL,
    -- The store's own multipart identifier. `NULL` on a local filesystem, and
    -- until the segment is opened.
    multipart_id  TEXT,
    -- Indexed by the sweep, so it needs a length `InnoDB` can key on.
    state         VARCHAR(32) NOT NULL,
    created_at    DATETIME(6) NOT NULL,
    updated_at    DATETIME(6) NOT NULL,
    -- One segment per position, and no way to write two.
    PRIMARY KEY (upload_id, seq),
    CONSTRAINT fk_upload_segments_upload FOREIGN KEY (upload_id)
        REFERENCES uploads (id) ON DELETE CASCADE
);

-- The sweep asks "which segments still hold something" across all sessions,
-- so the state is indexed, not the session.
CREATE INDEX idx_upload_segments_state ON upload_segments (state);

-- Every in-progress session becomes exactly one segment, pointing where it
-- already pointed. `backend` is nullable on `uploads` for sessions opened
-- before backends were recorded; those meant the `storage.url` backend, which
-- is the name the configuration seeds.
--
-- All `open`: a session commits its destination on opening, whether or not
-- the store returns an upload id. A local filesystem never does, so deriving
-- the state from that column would mark every local session as unstarted.
INSERT INTO upload_segments (
    upload_id, seq, backend, object_key, offset_bytes, length_bytes,
    multipart_id, state, created_at, updated_at
)
SELECT u.id, 0, COALESCE(u.backend, 'default'), u.object_key, 0, u.declared_bytes,
       u.multipart_id, 'open',
       u.created_at, u.updated_at
  FROM uploads u;

-- The three columns the list replaces. Kept until now so the backfill above had
-- something to read; leaving them would mean two places recording the same
-- thing and one of them going stale the first time a session has two segments.
ALTER TABLE uploads DROP COLUMN multipart_id;
ALTER TABLE uploads DROP COLUMN object_key;
ALTER TABLE uploads DROP COLUMN backend;
