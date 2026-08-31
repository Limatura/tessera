-- Acting on a provider telling us somebody has signed out.
--
-- # Why a column rather than reusing `origin`
--
-- `sessions.origin` already answers "who started this": a password here, or a
-- provider. That is enough when a logout token names a `sub`, because the
-- answer is "end every session this account has that a provider started".
--
-- It is not enough when the token names a `sid`. That is the provider's own
-- identifier for one of that person's sessions, and it is how a provider says
-- "this browser, not their phone". Acting on it means having written down which
-- of our sessions corresponds to which of theirs, and nothing did.
--
-- Nullable, because most sessions have no such thing: every password sign-in,
-- and every provider that does not release `sid`.
ALTER TABLE sessions ADD COLUMN provider_sid TEXT;

-- Finding the session a logout token names. Sparse, and read only on a logout.
-- MySQL cannot index a `TEXT` column without a prefix length, and has no
-- `CREATE INDEX IF NOT EXISTS`. 191 is the longest prefix that fits an index
-- entry under `utf8mb4`.
CREATE INDEX idx_sessions_provider_sid ON sessions (provider_sid(191));

-- `sessions.origin` needs nothing done to it. It already holds `oidc`, which is
-- also what `identities.provider` holds, because there is one provider and that
-- is its name. A logout token from one provider must not end sessions another
-- one started, so the day a second provider is configured both columns have to
-- carry its name instead. They line up today; that is worth knowing rather than
-- rediscovering.
