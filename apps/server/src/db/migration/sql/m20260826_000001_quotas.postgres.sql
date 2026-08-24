-- Per-role and per-account storage quotas.
--
-- # Three layers, and why
--
-- `storage.max_total_bytes` is one number for the whole instance, so the only
-- way to give one person more room was to give everybody more. A role covers
-- the ordinary case — members get this much, guests get that much — and an
-- account-level value covers the one person who needs different treatment
-- without inventing a role just for them.
--
-- Resolved highest-first: the account's own value, then its role's, then the
-- instance ceiling.
--
-- # NULL is not zero
--
-- NULL means "not set here, ask the next layer down". Zero means "no ceiling",
-- which is what `storage.max_total_bytes` has always meant and what the panel
-- shows. Without the distinction, an account set to 0 could not be told from
-- one nobody has set, and the two are opposite instructions.
--
-- # Why `uploads` gains a role too
--
-- An upload in flight reserves its declared size against a quota, and which
-- quota depends on who is uploading. Summing every open session would charge a
-- member for room a guest is holding. The column mirrors `shares.role_id` and
-- is written at session creation for the same reason: it is the role at the
-- time, not a pointer to whatever the account is in now.
ALTER TABLE roles ADD COLUMN quota_bytes BIGINT;
ALTER TABLE users ADD COLUMN quota_bytes BIGINT;
ALTER TABLE uploads ADD COLUMN role_id TEXT REFERENCES roles (id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS idx_uploads_role ON uploads (role_id);
