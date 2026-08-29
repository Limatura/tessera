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
    -- Which provider vouched for this. One today; the column is here so that a
    -- second one needs configuration rather than a migration.
    provider   TEXT NOT NULL,
    -- The provider's own identifier for the person: the `sub` claim.
    subject    TEXT NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    -- One account per subject per provider, and no way to write two.
    PRIMARY KEY (provider, subject)
);

-- Finding every way one account can sign in, which is what unlinking has to
-- count before it removes one.
CREATE INDEX IF NOT EXISTS idx_identities_user ON identities (user_id);

-- Where a sign-in that has left for the provider keeps what it will need when
-- the browser comes back.
--
-- `state` is the CSRF token: the callback is a request we did not make, and
-- this is what says it belongs to a redirect we did. `nonce` binds the returned
-- token to this attempt. `verifier` is the PKCE secret whose challenge went out
-- with the redirect.
--
-- A table rather than a signed cookie. Both work; a cookie needs `SameSite=Lax`
-- exactly, because `Strict` withholds it on the top-level navigation back from
-- the provider, and that failure looks like an unrelated error to whoever hits
-- it. Rows are cheap, the sweep already clears four other short-lived tables,
-- and the redirect endpoint is throttled like the other unauthenticated ones.
CREATE TABLE IF NOT EXISTS oidc_flows (
    state       TEXT NOT NULL PRIMARY KEY,
    nonce       TEXT NOT NULL,
    verifier    TEXT NOT NULL,
    -- `login` or `link`. The callback is one endpoint and this is how it knows
    -- whether it is signing somebody in or attaching a provider to the account
    -- that is already signed in.
    purpose     TEXT NOT NULL,
    -- Set when `purpose` is `link`: whose account this will be attached to.
    user_id     TEXT REFERENCES users (id) ON DELETE CASCADE,
    -- Carried through the provider and back, so an invitation can be spent by
    -- somebody who has never had a password here. The provider never sees it.
    invite_code TEXT,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oidc_flows_expires ON oidc_flows (expires_at);

-- Passwords move out of `users` and into a table of their own.
--
-- # Why not a nullable column
--
-- The obvious change is `users.password_hash` becoming nullable. It cannot be
-- done here. SQLite has no `DROP NOT NULL`, so the table would have to be
-- rebuilt, and seven tables carry a foreign key to `users`:
--
--   * renaming `users` out of the way rewrites all seven to point at the
--     temporary name, and they dangle the moment it is dropped;
--   * dropping `users` first fires `ON DELETE CASCADE` on every one of them,
--     which empties `sessions`, `shares` and `api_tokens` — measured, not
--     feared;
--   * and renaming a new table into the freed name fails outright while those
--     seven still reference it.
--
-- `PRAGMA foreign_keys = OFF` would suppress the first and second, and it is a
-- connection setting these statements are not guaranteed to share.
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
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

INSERT INTO credentials (user_id, password_hash, created_at, updated_at)
SELECT id, password_hash, created_at, updated_at FROM users;

-- Dropping a plain column needs no rebuild, and nothing indexes this one.
ALTER TABLE users DROP COLUMN password_hash;

ALTER TABLE sessions ADD COLUMN origin TEXT NOT NULL DEFAULT 'password';
