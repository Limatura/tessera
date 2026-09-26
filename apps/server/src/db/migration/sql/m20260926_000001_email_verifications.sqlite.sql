-- Links that prove an account can receive mail at its address.
--
-- Registration proves it once, by making the account wait for the link. An
-- address changed afterwards, or an account made by an administrator or before
-- mail was configured, has no such proof, and this is how it gets one.
--
-- The same shape as password_reset_tokens, for the same reasons: a row so a
-- link can be spent and revoked, only the digest stored so a stolen database
-- holds nothing that can be presented, and used_at kept rather than the row
-- deleted so "already used" and "never existed" stay apart.
--
-- # Why the address is on the row
--
-- A link proves the address it was sent to, not whatever the account's address
-- is by the time it is clicked. Without the column, asking for a link and then
-- changing the address to one you cannot receive would verify the new one with
-- the old one's mail.
CREATE TABLE IF NOT EXISTS email_verifications (
    id          TEXT NOT NULL PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    email       TEXT NOT NULL,
    secret_hash TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    used_at     TEXT
);

CREATE INDEX IF NOT EXISTS idx_email_verifications_user ON email_verifications (user_id);
CREATE INDEX IF NOT EXISTS idx_email_verifications_expires_at ON email_verifications (expires_at);
