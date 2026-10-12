-- Undoing the OIDC schema.
--
-- The column comes back and the passwords go into it. An account with no row
-- in `credentials` had no password, which `NOT NULL` cannot hold, so it is
-- removed: rolling back a sign-in method removes the accounts that only had
-- that method.
DROP TABLE IF EXISTS oidc_flows;
DROP TABLE IF EXISTS identities;

DELETE FROM users WHERE id NOT IN (SELECT user_id FROM credentials);

-- Added with a default so that the existing rows satisfy `NOT NULL` while the
-- real values are copied in. The default is not a usable hash and nothing can
-- present a password that matches it.
ALTER TABLE users ADD COLUMN password_hash TEXT NOT NULL DEFAULT '';

UPDATE users
SET password_hash = (SELECT password_hash FROM credentials WHERE credentials.user_id = users.id);

DROP TABLE credentials;

ALTER TABLE sessions DROP COLUMN origin;
