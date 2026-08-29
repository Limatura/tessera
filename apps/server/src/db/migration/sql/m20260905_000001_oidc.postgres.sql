-- Signing in with an identity provider instead of a password.
--
-- Three changes, and each of them is here because the alternative is worse
-- later rather than because OIDC needs it today.
--
-- # Why `identities` and not a column on `users`
--
-- One account may end up with several ways to sign in: a password it was
-- created with, an identity from the company's Authentik, and later perhaps a
-- second provider. A column pair on `users` says "one provider, for ever" and
-- has to be migrated the first time that is wrong.
--
-- `subject` is the provider's own identifier for the person. It is stable and
-- never reused within one issuer, which is what makes it safe to key on. The
-- e-mail address is not: providers let people change it, and some do not
-- verify it at all.
--
-- # Why the e-mail address is still required
--
-- The `email` scope is optional in the specification, so a provider need not
-- release one. Rather than making the column nullable, a sign-in that arrives
-- without an address is refused with a message naming the scope to grant.
--
-- The column is `NOT NULL` and uniquely indexed, and everything from password
-- reset to notification assumes an address exists. Relaxing that to serve a
-- provider that can be configured to release one is a large change for a small
-- case: Authentik, Authelia, Keycloak, Google and GitHub all can.
--
-- # Why `sessions.origin`
--
-- A provider can disable somebody, and a session that outlives that is exactly
-- what single sign-on exists to prevent. What can be done about it depends on
-- what the provider supports, so a session has to know where it came from:
-- password sessions keep the configured lifetime, provider sessions may be
-- given a shorter one or revalidated.
--
-- Written from the first day, unlike a column added in case it is wanted.

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

-- Passwords move out of `users` and into a table of their own.
--
-- # Why not a nullable column
--
-- This dialect could make `users.password_hash` nullable in one statement.
-- SQLite cannot, and the three dialects have to end up with the same schema.
-- The SQLite file explains what goes wrong there.
--
-- # Why this shape is better anyway
--
-- `identities` is already one row per way of signing in. A password as a column
-- on `users` makes one method a column and the other a row. As a table, "no
-- row" means "no password" and needs no explanation, which is exactly what an
-- account that has only ever used a provider is.
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
