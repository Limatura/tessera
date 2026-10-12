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
    -- The types the OIDC migration used for the same columns. `TEXT` cannot be
    -- a key or an index here without a prefix length, and `DATETIME(6)` is what
    -- every other table in this schema stores a time as.
    state       VARCHAR(191) NOT NULL PRIMARY KEY,
    -- Which provider this flow is for, matching `identities.provider`. The
    -- callback is one endpoint for all of them and this is how it knows whose
    -- token endpoint to go to.
    provider    VARCHAR(64)  NOT NULL,
    -- Absent for a provider that issues no ID token. See the SQLite file.
    nonce       TEXT,
    verifier    TEXT         NOT NULL,
    -- `login` or `link`.
    purpose     VARCHAR(32)  NOT NULL,
    -- Set when `purpose` is `link`: whose account this will be attached to.
    user_id     VARCHAR(191),
    -- Carried through the provider and back, so an invitation can be used by
    -- someone who has never had a password here. The provider never sees it.
    invite_code TEXT,
    created_at  DATETIME(6)  NOT NULL,
    expires_at  DATETIME(6)  NOT NULL,
    -- A column-level `REFERENCES` is parsed and ignored by InnoDB.
    CONSTRAINT fk_auth_flows_user FOREIGN KEY (user_id)
        REFERENCES users (id) ON DELETE CASCADE
);

-- MySQL has no `CREATE INDEX IF NOT EXISTS`.
CREATE INDEX idx_auth_flows_expires ON auth_flows (expires_at);
