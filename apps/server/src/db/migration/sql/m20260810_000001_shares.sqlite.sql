-- Shares: what this service exists to hand out.
--
-- # Why two tables
--
-- The access gate runs on every public request and needs six small columns:
-- `code`, `password_hash`, `use_limit`, `use_count`, `expires_at`, `disabled_at`.
-- The content bytes are only needed when something is actually sent.
--
-- In one table, `SELECT code, expires_at FROM shares` would pull pages holding
-- 64 KiB blobs into memory. So the split is by *size and access frequency*,
-- not by kind: `shares` stays narrow and hot, and `share_contents` is read only
-- by `/raw` and `/download`.
--
-- By the same rule, `filename` and `language` stay here even though each is
-- meaningless for one of the two kinds. They are small, and the metadata
-- endpoint returns both kinds; a table per kind would buy tidiness at the cost
-- of a join on the common path.
CREATE TABLE IF NOT EXISTS shares (
    id             TEXT    NOT NULL PRIMARY KEY,
    -- The public retrieval code, as it appears in `/s/{code}`. Separate from the
    -- primary key so it can be rotated without rewriting foreign keys, and so
    -- inserts keep the locality a ULID primary key gives us.
    code           TEXT    NOT NULL UNIQUE,
    -- NULL for an anonymous upload. `ON DELETE SET NULL`, not CASCADE: what
    -- happens to a departing account's shares is not decided yet, and orphaning a
    -- row is recoverable while deleting it is not. Deleting would also strand the
    -- objects, since no database action reaches the blob store.
    user_id        TEXT    REFERENCES users (id) ON DELETE SET NULL,
    kind           TEXT    NOT NULL,

    -- Everything the access gate reads.
    password_hash  TEXT,
    -- NULL means unlimited. A count plus a limit instead of a countdown: a
    -- countdown discards how many times something was actually fetched, and that
    -- number cannot be reconstructed later.
    use_limit      INTEGER,
    use_count      INTEGER NOT NULL DEFAULT 0,
    expires_at     TEXT,
    disabled_at    TEXT,

    -- Presentation.
    title          TEXT,
    filename       TEXT,
    -- Recorded, never echoed. The uploader chooses this value, and serving it
    -- back is how a text/html upload becomes stored XSS on our own origin.
    content_type   TEXT,
    language       TEXT,
    size_bytes     INTEGER NOT NULL,

    -- Where the bytes live. `object_key` is stored, not derived from the id and
    -- date, so the layout can change later without moving a byte: old rows keep
    -- the key they were written with.
    storage        TEXT    NOT NULL,
    object_key     TEXT,

    og_config      TEXT,
    -- Lets an anonymous uploader delete what they just posted. Without it a
    -- mistaken upload is permanent.
    management_token_hash TEXT,
    view_count     INTEGER NOT NULL DEFAULT 0,
    last_accessed_at TEXT,

    created_at     TEXT    NOT NULL,
    updated_at     TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_shares_user ON shares (user_id);
CREATE INDEX IF NOT EXISTS idx_shares_expires_at ON shares (expires_at);

-- Inline content, for shares small enough that a round trip to the blob store
-- costs more than it saves. Anything larger carries an `object_key` instead and
-- has no row here.
CREATE TABLE IF NOT EXISTS share_contents (
    share_id TEXT NOT NULL PRIMARY KEY REFERENCES shares (id) ON DELETE CASCADE,
    content  BLOB NOT NULL
);
