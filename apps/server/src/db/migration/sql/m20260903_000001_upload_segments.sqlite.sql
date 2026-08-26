-- Where an upload in flight is putting its bytes, as a list rather than three
-- columns.
--
-- # What the columns could not say
--
-- `uploads` carried `backend`, `object_key` and `multipart_id`: one destination,
-- decided when the session opened. A file that does not fit in any single
-- backend has to be written to several, one after another, and there is no way
-- to spell "the first one is finished, the second has not been opened yet" in
-- three singular columns. That sentence is this table.
--
-- Every session has exactly one row here today, so nothing behaves differently
-- — but arriving at the list later would mean migrating sessions that are
-- mid-flight, and arriving at it now costs one migration nobody has to repeat.
--
-- # Why `state` and not a pair of nullable columns
--
-- "Has a multipart id" and "has been written to" are not the same question, and
-- neither of them is "has this segment finished". A local filesystem has no
-- multipart id at all, so inferring the state from that column would make every
-- local segment look unopened for ever. Three named states say it once.
--
--   planned  — the space is spoken for and nothing has been written
--   open     — the destination is committed and bytes may go in
--   complete — every byte of this segment is in the store
--
-- `planned` has no writer yet, and that is deliberate rather than an oversight:
-- it is the reservation, and nothing reserves until the planner spreads a file
-- over more than one backend. Two large uploads arriving together must not both
-- be planned onto the same backend's last free gigabyte, and a row that exists
-- before the first byte is what will stop that. Every session today commits its
-- destination the moment it opens, so every row is `open`.
--
-- # Why the sweep cares
--
-- A `complete` segment is holding real space in a real bucket while its share
-- does not exist yet. Nothing used to be able to see that: the space was spent,
-- the planner could not count it, and a free-tier bucket filled up with no
-- query able to explain why. Cleaning up now means aborting every `open` row
-- and deleting the object behind every `complete` one — the same contract as
-- before, once per segment.
CREATE TABLE IF NOT EXISTS upload_segments (
    upload_id     TEXT      NOT NULL REFERENCES uploads (id) ON DELETE CASCADE,
    -- Position in the file, from zero. One row today, so always 0.
    seq           INTEGER   NOT NULL DEFAULT 0,
    -- Which configured backend this piece goes to.
    backend       TEXT      NOT NULL,
    object_key    TEXT      NOT NULL,
    -- Where this piece starts within the whole file, and how long it will be.
    -- How much has actually landed is not here: `uploads.offset_bytes` already
    -- says it for the whole file, and a second copy is a second thing to keep
    -- true.
    offset_bytes  INTEGER   NOT NULL DEFAULT 0,
    length_bytes  INTEGER   NOT NULL,
    -- The store's own multipart identifier. `NULL` on a local filesystem, and
    -- until the segment is opened.
    multipart_id  TEXT,
    state         TEXT      NOT NULL,
    created_at    TEXT      NOT NULL,
    updated_at    TEXT      NOT NULL,
    -- One segment per position, and no way to write two.
    PRIMARY KEY (upload_id, seq)
);

-- The sweep asks "which segments still hold something", across every session at
-- once, so the state is the thing to index rather than the session.
CREATE INDEX IF NOT EXISTS idx_upload_segments_state ON upload_segments (state);

-- Every session in flight becomes exactly one segment, pointing where it was
-- already pointing. `backend` is nullable on `uploads` for sessions opened
-- before backends were recorded; those meant the one from `storage.url`, which
-- is the name the configuration seeds.
--
-- All `open`: a session commits its destination when it opens, whether or not
-- the store hands back an upload id — a local filesystem never does, so keying
-- the state off that column would file every local session as unstarted.
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
