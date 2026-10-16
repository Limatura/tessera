-- See the SQLite file.
--
-- The `DELETE` uses a join instead of a subquery on `users`: MySQL refuses to
-- read the table it is updating through one.
DROP TABLE IF EXISTS oidc_flows;
DROP TABLE IF EXISTS identities;

DELETE users FROM users
LEFT JOIN credentials ON credentials.user_id = users.id
WHERE credentials.user_id IS NULL;

ALTER TABLE users ADD COLUMN password_hash VARCHAR(255) NOT NULL DEFAULT '';

UPDATE users
JOIN credentials ON credentials.user_id = users.id
SET users.password_hash = credentials.password_hash;

DROP TABLE credentials;

ALTER TABLE sessions DROP COLUMN origin;
