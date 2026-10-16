-- See the SQLite file.
DROP TABLE IF EXISTS oidc_flows;
DROP TABLE IF EXISTS identities;

DELETE FROM users WHERE id NOT IN (SELECT user_id FROM credentials);

ALTER TABLE users ADD COLUMN password_hash TEXT NOT NULL DEFAULT '';

UPDATE users
SET password_hash = credentials.password_hash
FROM credentials
WHERE credentials.user_id = users.id;

DROP TABLE credentials;

ALTER TABLE sessions DROP COLUMN IF EXISTS origin;
