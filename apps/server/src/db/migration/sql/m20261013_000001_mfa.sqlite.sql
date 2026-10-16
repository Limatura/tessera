-- A second step at sign-in: an authenticator app, and codes for when the phone
-- is gone.
--
-- # Why the TOTP secret is sealed and not hashed
--
-- Checking a code means computing it, and computing it needs the secret
-- itself. So it has to be recoverable, which makes it the same kind of value
-- as a backend URL: sealed with `security.secret_key`, so a database dump
-- alone holds ciphertext and not a way to generate codes.
--
-- # Why `confirmed_at` can be empty
--
-- Setting up writes the row before the app has proven it scanned the secret.
-- Until a code from it has been accepted, the account must not ask for one at
-- sign-in: a mistyped QR scan would otherwise lock the owner out on the spot.
--
-- # Why `last_used_step` exists
--
-- A code stays valid for its whole thirty-second step, and a step either side
-- is accepted for clock drift. Recording the step that was used refuses the
-- same code a second time, so one read over a shoulder cannot be replayed.
CREATE TABLE IF NOT EXISTS totp_authenticators (
    user_id        TEXT    NOT NULL PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    secret         TEXT    NOT NULL,
    confirmed_at   TEXT,
    last_used_step INTEGER,
    created_at     TEXT    NOT NULL
);

-- One-time codes, for an account whose authenticator is lost.
--
-- Only the digest is stored, for the same reason as a session's: the rows
-- must not be usable as credentials. `used_at` is kept instead of deleting the
-- row so the count left can be shown without a second table.
CREATE TABLE IF NOT EXISTS recovery_codes (
    id         TEXT NOT NULL PRIMARY KEY,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    code_hash  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    used_at    TEXT
);

CREATE INDEX IF NOT EXISTS idx_recovery_codes_user ON recovery_codes (user_id);

-- A sign-in that has passed the password and waits for the second step.
--
-- A row, not a session with a flag, so nothing that accepts a session can
-- mistake a half-finished sign-in for a finished one. Keyed on the digest of
-- the ticket the client holds, and deleted when used, so a ticket works once.
CREATE TABLE IF NOT EXISTS mfa_challenges (
    secret_hash TEXT NOT NULL PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_mfa_challenges_expires_at ON mfa_challenges (expires_at);
