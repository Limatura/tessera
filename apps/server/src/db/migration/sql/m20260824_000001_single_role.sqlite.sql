-- One role per account.
--
-- # Why the join table goes
--
-- Nothing needed more than one. The seeded roles are nested (`member` is a
-- subset of `admin`, which is a subset of `owner`), so holding two means
-- holding the larger. No code relied on a union of more than one row:
-- `permissions_of` collected a set, and a set of one is that one.
--
-- The cost was every place that wanted a single answer from a plural fact:
-- which role a share counts against, which quota applies, which role an
-- identity provider's groups map to.
--
-- # Which one is kept
--
-- The API accepted a list, so one row per account is not guaranteed. With more
-- than one, the role with the most permissions wins: removing it would change
-- the most, and any other choice could take something away.
--
-- Ties break on `role_id`, so all three dialects agree and reruns give the
-- same answer.
--
-- # Nullable
--
-- An account in no role is a valid state, not a fault: signed in and allowed
-- nothing. It was expressible before (no rows) and stays expressible.
--
-- `ON DELETE SET NULL` matches the join table's `ON DELETE CASCADE` on
-- `role_id`: removing a role removes the assignment, not the account.
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
