-- Password reset links.
--
-- # Why a row and not a signed value
--
-- The same reasons as share_tickets, plus one more: a reset token must be
-- revocable. Using it must invalidate it, and so must changing the password by
-- any other route; otherwise a link left in a mailbox stays a working key to
-- the account long after the emergency it was issued for.
--
-- A signed token cannot be revoked without a revocation list, which is a table
-- again, with the expiry logic split across two places.
--
-- Only the digest is stored, as with sessions, API tokens and share tickets: a
-- stolen database contains nothing that can be presented. The link in the
-- email is the only copy of the secret.
--
-- # Why used_at instead of deleting the row
--
-- Deleting on use makes "this link was already used" and "this link never
-- existed" indistinguishable, and they deserve different messages: the first
-- means check your inbox for a newer mail, the second means someone is
-- guessing. The sweep removes rows once they expire.
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id          TEXT NOT NULL PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    secret_hash TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    used_at     TEXT
);

CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_user ON password_reset_tokens (user_id);
CREATE INDEX IF NOT EXISTS idx_password_reset_tokens_expires_at ON password_reset_tokens (expires_at);
