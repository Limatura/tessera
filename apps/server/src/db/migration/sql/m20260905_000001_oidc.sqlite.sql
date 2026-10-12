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
    -- The provider that vouched for this. Only one today; the column exists so a
    -- second one needs configuration instead of a migration.
    provider   TEXT NOT NULL,
    -- The provider's own identifier for the person: the `sub` claim.
    subject    TEXT NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    -- One account per subject per provider, and no way to write two.
    PRIMARY KEY (provider, subject)
);

-- Finding every sign-in method of an account, which unlinking must count
-- before removing one.
CREATE INDEX IF NOT EXISTS idx_identities_user ON identities (user_id);

-- Where a sign-in that has gone to the provider keeps what it will need when
-- the browser returns.
--
-- `state` is the CSRF token: the callback is a request we did not make, and
-- this ties it to a redirect we did. `nonce` binds the returned token to this
-- attempt. `verifier` is the PKCE secret whose challenge went out with the
-- redirect.
--
-- A table instead of a signed cookie. Both work, but a cookie needs exactly
-- `SameSite=Lax`, since `Strict` withholds it on the top-level navigation back
-- from the provider, and that failure looks like an unrelated error. Rows are
-- cheap, the sweep already clears four other short-lived tables, and the
-- redirect endpoint is throttled like the other unauthenticated ones.
CREATE TABLE IF NOT EXISTS oidc_flows (
    state       TEXT NOT NULL PRIMARY KEY,
    nonce       TEXT NOT NULL,
    verifier    TEXT NOT NULL,
    -- `login` or `link`. The callback is one endpoint, and this tells it whether
    -- it is signing a user in or linking a provider to the signed-in account.
    purpose     TEXT NOT NULL,
    -- Set when `purpose` is `link`: whose account this will be attached to.
    user_id     TEXT REFERENCES users (id) ON DELETE CASCADE,
    -- Carried through the provider and back, so an invitation can be used by
    -- someone who has never had a password here. The provider never sees it.
    invite_code TEXT,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oidc_flows_expires ON oidc_flows (expires_at);

-- Passwords move out of `users` into their own table.
--
-- # Why not a nullable column
--
-- The obvious change is making `users.password_hash` nullable, which is not
-- possible here. SQLite has no `DROP NOT NULL`, so the table would have to be
-- rebuilt, and seven tables have a foreign key to `users`:
--
--   * renaming `users` out of the way repoints all seven at the temporary
--     name, and they dangle once it is dropped;
--   * dropping `users` first fires `ON DELETE CASCADE` on all of them, which
--     empties `sessions`, `shares` and `api_tokens` (tested, not assumed);
--   * renaming a new table into the freed name fails outright while those
--     seven still reference it.
--
-- `PRAGMA foreign_keys = OFF` would avoid the first two, but it is a
-- connection setting these statements are not guaranteed to share.
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
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

INSERT INTO credentials (user_id, password_hash, created_at, updated_at)
SELECT id, password_hash, created_at, updated_at FROM users;

-- Dropping a plain column needs no rebuild, and nothing indexes this one.
ALTER TABLE users DROP COLUMN password_hash;

ALTER TABLE sessions ADD COLUMN origin TEXT NOT NULL DEFAULT 'password';
