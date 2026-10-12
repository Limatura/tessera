-- Acting on a provider's notice that a user has signed out.
--
-- # Why a column instead of reusing `origin`
--
-- `sessions.origin` already answers "who started this": a password here, or a
-- provider. That is enough when a logout token names a `sub`, because the
-- action is "end every provider-started session of this account".
--
-- It is not enough when the token names a `sid`: the provider's own id for one
-- of the user's sessions, its way of saying "this browser, not their phone".
-- Acting on it requires recording which of our sessions matches which of
-- theirs, and nothing did.
--
-- Nullable, because most sessions have none: every password sign-in, and
-- every provider that does not release `sid`.
ALTER TABLE sessions ADD COLUMN provider_sid TEXT;

-- Finding the session a logout token names. Sparse, and read only on logout.
-- MySQL cannot index a `TEXT` column without a prefix length, and has no
-- `CREATE INDEX IF NOT EXISTS`. 191 is the longest prefix that fits an index
-- entry under `utf8mb4`.
CREATE INDEX idx_sessions_provider_sid ON sessions (provider_sid(191));

-- `sessions.origin` needs no change. It already holds `oidc`, which is also
-- what `identities.provider` holds, since there is one provider with that
-- name. A logout token from one provider must not end sessions another
-- started, so once a second provider is configured both columns must carry
-- its name instead. They line up today; noting it saves rediscovering it.
