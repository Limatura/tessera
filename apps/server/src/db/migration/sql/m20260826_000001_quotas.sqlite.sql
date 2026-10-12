-- Per-role and per-account storage quotas.
--
-- # Three layers, and why
--
-- `storage.max_total_bytes` is one number for the whole instance, so the only
-- way to give one account more room was to give everyone more. A role covers
-- the usual case (members get this much, guests that much), and a per-account
-- value covers the one user who needs different treatment without a role
-- invented just for them.
--
-- Resolved most specific first: the account's own value, then its role's,
-- then the instance limit.
--
-- # NULL is not zero
--
-- NULL means "not set here, ask the next layer". Zero means "no limit", which
-- is what `storage.max_total_bytes` has always meant and what the panel shows.
-- Without the distinction, an account set to 0 would be indistinguishable
-- from one never set, and they are opposite instructions.
--
-- # Why `uploads` gets a role too
--
-- An upload in progress reserves its declared size against a quota, and which
-- quota depends on the uploader. Summing every open session would charge a
-- member for space a guest is holding. The column mirrors `shares.role_id` and
-- is written at session creation for the same reason: it is the role at that
-- time, not a pointer to the account's current role.
ALTER TABLE roles ADD COLUMN quota_bytes INTEGER;
ALTER TABLE users ADD COLUMN quota_bytes INTEGER;
ALTER TABLE uploads ADD COLUMN role_id TEXT REFERENCES roles (id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_uploads_role ON uploads (role_id);
