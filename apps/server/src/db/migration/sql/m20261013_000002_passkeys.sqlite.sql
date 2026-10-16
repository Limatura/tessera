-- Passkeys: signing in with a fingerprint, a face or a hardware key instead of
-- a password.
--
-- # What is stored
--
-- The credential id the authenticator chose, and its public key exactly as it
-- sent it: a COSE structure, the format the specification defines rather than
-- any library's. Neither is secret. A public key verifies and cannot sign, so a
-- dump of this table lets nobody in.
--
-- # Why the id is looked up by its digest
--
-- An id is up to 1023 bytes, which is more than MySQL will put in a unique
-- index as text. The SHA-256 of it is a fixed 64 characters that every backend
-- indexes the same way, and it is what a sign-in looks the passkey up by.
--
-- # The counter
--
-- An authenticator that keeps one increments it on every use, so a value that
-- goes backwards means two copies of one key. Most passkeys sync between
-- devices and keep it at zero, which says nothing either way.
CREATE TABLE IF NOT EXISTS passkeys (
    id              TEXT    NOT NULL PRIMARY KEY,
    user_id         TEXT    NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    credential_hash TEXT    NOT NULL UNIQUE,
    credential_id   TEXT    NOT NULL,
    public_key      TEXT    NOT NULL,
    sign_count      INTEGER NOT NULL,
    name            TEXT    NOT NULL,
    backup_eligible INTEGER NOT NULL,
    backed_up       INTEGER NOT NULL,
    created_at      TEXT    NOT NULL,
    last_used_at    TEXT
);

CREATE INDEX IF NOT EXISTS idx_passkeys_user ON passkeys (user_id);

-- A challenge handed to a browser and not yet answered.
--
-- The same arrangement as `mfa_challenges`: keyed on the digest of what the
-- browser holds, and deleted when answered, so an answer works once. `purpose`
-- keeps a sign-in's challenge from being spent registering a key, and
-- `user_id` says whose account a registration adds to.
CREATE TABLE IF NOT EXISTS webauthn_challenges (
    secret_hash TEXT NOT NULL PRIMARY KEY,
    purpose     TEXT NOT NULL,
    user_id     TEXT REFERENCES users (id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_webauthn_challenges_expires_at ON webauthn_challenges (expires_at);
