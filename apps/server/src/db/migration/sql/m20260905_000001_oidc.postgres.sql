-- Signing in with an identity provider instead of a password.
--
-- Three changes, each made now because the alternative is worse later, not
-- because OIDC needs it today.
--
-- # Why `identities` and not a column on `users`
--
-- An account may end up with several ways to sign in: the password it was
-- created with, an identity from the company's Authentik, and later perhaps a
-- second provider. A column pair on `users` says "one provider, forever" and
-- needs a migration the first time that is wrong.
--
-- `subject` is the provider's own identifier for the user. It is stable and
-- never reused within one issuer, which makes it safe to key on. The email
-- address is not: providers let users change it, and some do not verify it.
--
-- # Why the email address is still required
--
-- The `email` scope is optional in the spec, so a provider need not release
-- one. Instead of making the column nullable, a sign-in without an address is
-- refused with a message naming the scope to grant.
--
-- The column is `NOT NULL` and uniquely indexed, and everything from password
-- reset to notifications assumes an address. Relaxing that for providers that
-- can be configured to release one anyway is a large change for a small case:
-- Authentik, Authelia, Keycloak, Google and GitHub all can.
--
-- # Why `sessions.origin`
--
-- A provider can disable a user, and a session outliving that is what single
-- sign-on exists to prevent. What can be done depends on the provider, so a
-- session must record where it came from: password sessions keep the
-- configured lifetime, provider sessions may get a shorter one or be
-- revalidated.
--
-- Written from day one, unlike a column added in case it is wanted.

CREATE TABLE IF NOT EXISTS identities (
    provider   TEXT NOT NULL,
    subject    TEXT NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (provider, subject)
);

CREATE INDEX IF NOT EXISTS idx_identities_user ON identities (user_id);

CREATE TABLE IF NOT EXISTS oidc_flows (
    state       TEXT NOT NULL PRIMARY KEY,
    nonce       TEXT NOT NULL,
    verifier    TEXT NOT NULL,
    purpose     TEXT NOT NULL,
    user_id     TEXT REFERENCES users (id) ON DELETE CASCADE,
    invite_code TEXT,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oidc_flows_expires ON oidc_flows (expires_at);

-- Passwords move out of `users` into their own table.
--
-- # Why not a nullable column
--
-- This dialect could make `users.password_hash` nullable in one statement.
-- SQLite cannot, and all three dialects must end up with the same schema. The
-- SQLite file explains what goes wrong there.
--
-- # Why this shape is better anyway
--
-- `identities` is already one row per sign-in method. A password column on
-- `users` would make one method a column and the other a row. As a table, "no
-- row" means "no password" without explanation, which describes an account
-- that has only ever used a provider.
CREATE TABLE IF NOT EXISTS credentials (
    user_id       TEXT NOT NULL PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    -- PHC-format Argon2 string.
    password_hash TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL,
    updated_at    TIMESTAMPTZ NOT NULL
);

INSERT INTO credentials (user_id, password_hash, created_at, updated_at)
SELECT id, password_hash, created_at, updated_at FROM users;

-- Dropping a plain column needs no rebuild, and nothing indexes this one.
ALTER TABLE users DROP COLUMN password_hash;

ALTER TABLE sessions ADD COLUMN IF NOT EXISTS origin TEXT NOT NULL DEFAULT 'password';
