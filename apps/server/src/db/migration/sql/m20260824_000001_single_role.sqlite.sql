-- One role per account.
--
-- # Why the join table goes
--
-- Nothing needed more than one. The seeded roles are nested — `member` is a
-- subset of `admin`, which is a subset of `owner` — so holding two of them is
-- holding the larger one. No code depended on the union being of more than one
-- row: `permissions_of` collected a set, and a set of one is that one.
--
-- What it cost was everywhere a single answer was wanted from a plural fact:
-- which role a share counts against, which quota applies, which role an
-- identity provider's groups map to.
--
-- # Which one is kept
--
-- The API accepted a list, so a row per account is not guaranteed. Where there
-- is more than one, the one with the most permissions wins — that is the role
-- whose removal would change the most, and every other choice can take
-- something away from somebody.
--
-- Ties break on `role_id` so the three dialects agree, and so re-running gives
-- the same answer.
--
-- # Nullable
--
-- An account in no role is a real state, not a fault: signed in and allowed
-- nothing. It was expressible before (no rows) and stays expressible.
--
-- `ON DELETE SET NULL` matches what the join table did with `ON DELETE
-- CASCADE` on `role_id` — removing a role takes the assignment, not the
-- account.
ALTER TABLE users ADD COLUMN role_id TEXT REFERENCES roles (id) ON DELETE SET NULL;

UPDATE users
   SET role_id = (
         SELECT ur.role_id
           FROM user_roles ur
           LEFT JOIN role_permissions rp ON rp.role_id = ur.role_id
          WHERE ur.user_id = users.id
          GROUP BY ur.role_id
          ORDER BY COUNT(rp.permission) DESC, ur.role_id ASC
          LIMIT 1
       );

DROP TABLE user_roles;

CREATE INDEX IF NOT EXISTS idx_users_role ON users (role_id);
