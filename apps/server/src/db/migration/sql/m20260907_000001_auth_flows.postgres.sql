-- A sign-in in progress at a provider, of any kind.
--
-- # Why the table is replaced instead of altered
--
-- Two changes are needed: a column for which provider a flow is for, and a
-- `nonce` that may be absent. The first is one statement. The second is not:
-- SQLite has no `DROP NOT NULL`, so relaxing a column means rebuilding the
-- table, where the pitfalls are.
--
-- There are none here, and it is worth saying why. Nothing has a foreign key
-- to this table; it has one *to* `users`, which is unaffected. And every row
-- is an unfinished sign-in that expires fifteen minutes after it started and
-- is swept by `maintenance`, so there is no data to preserve. Dropping it costs
-- anyone mid-sign-in during a deploy one more click.
--
-- # Why a nonce can be absent
--
-- It is an OpenID Connect claim. Plain OAuth 2.0 has no ID token to carry
-- one, so a GitHub or Discord flow has nothing to store or compare. The
-- callback is then protected by `state`, which this table makes one-shot by
-- deleting the row, and by PKCE, whose verifier never leaves here.
--
-- Storing an empty string instead would make "this provider has no nonce"
-- and "this provider sent an empty one" the same row, and the second must be
-- refused.
DROP TABLE IF EXISTS oidc_flows;

CREATE TABLE IF NOT EXISTS auth_flows (
    state       TEXT NOT NULL PRIMARY KEY,
    -- Which provider this flow is for, matching `identities.provider`. The
    -- callback is one endpoint for all of them and this is how it knows whose
    -- token endpoint to go to.
    provider    TEXT NOT NULL,
    -- Absent for a provider that issues no ID token. See above.
    nonce       TEXT,
    verifier    TEXT NOT NULL,
    -- `login` or `link`.
    purpose     TEXT NOT NULL,
    -- Set when `purpose` is `link`: whose account this will be attached to.
    user_id     TEXT REFERENCES users (id) ON DELETE CASCADE,
    -- Carried through the provider and back, so an invitation can be used by
    -- someone who has never had a password here. The provider never sees it.
    invite_code TEXT,
    -- The types the OIDC migration used for the same columns. A time in a `text`
    -- column is a string, and `expires_at <= now` would compare lexicographically
    -- instead of comparing times.
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_auth_flows_expires ON auth_flows (expires_at);
