-- Signing in a program that has no browser, such as the command line client,
-- by approving it from one that does (RFC 8628, the device authorization
-- grant).
--
-- The program asks for a row and gets two codes back. It keeps the device code
-- and polls with it; the person types the short user code into the site, where
-- they are signed in, and approves. The next poll turns the row into an API
-- token and deletes it.
--
-- Only the device code's digest is stored, like every other secret here. The
-- user code is stored as it is: it has to be looked up by what a person types,
-- it lives for minutes, and on its own it can only approve a request, never
-- redeem one.
--
-- # Why the token is minted on redemption, not on approval
--
-- Minting it on approval would mean keeping the secret somewhere until the
-- program comes back for it. Recording who approved and with which permissions
-- keeps nothing presentable in the table.
CREATE TABLE IF NOT EXISTS device_authorizations (
    device_code_hash TEXT NOT NULL PRIMARY KEY,
    user_code        TEXT NOT NULL UNIQUE,
    client_name      TEXT NOT NULL,
    requested_from   TEXT NOT NULL,
    user_id          TEXT REFERENCES users (id) ON DELETE CASCADE,
    permissions      TEXT,
    approved_at      TEXT,
    denied_at        TEXT,
    created_at       TEXT NOT NULL,
    expires_at       TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_device_authorizations_expires_at ON device_authorizations (expires_at);
