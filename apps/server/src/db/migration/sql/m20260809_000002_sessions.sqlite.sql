-- Browser sessions.
--
-- `secret_hash` is a SHA-256 digest, never the secret itself: a stolen database
-- must not contain anything that can be presented as a credential. The secret is
-- 256 bits of OS randomness, so a fast hash is the right one here — there is
-- nothing to brute-force, and paying Argon2 on every authenticated request would
-- make the service unusable.
--
-- `ON DELETE CASCADE` so deleting an account really ends its sessions. SQLite
-- only enforces this when `PRAGMA foreign_keys` is on, which db::connect_sqlite
-- sets on every connection.
CREATE TABLE IF NOT EXISTS sessions (
    id           TEXT NOT NULL PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    secret_hash  TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    expires_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_sessions_user ON sessions (user_id);

-- The expiry sweep scans by this column, so it is worth an index even though the
-- table is small: it stays small precisely because the sweep is cheap.
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions (expires_at);
