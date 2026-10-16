-- A role with no permissions.
--
-- # Why such a role is worth a row
--
-- Not for what it grants. Retrieval never looks at an account (a code, a
-- password and a ticket are all it needs), so "may download but not upload"
-- already describes someone without an account, and needs no role.
--
-- It is for `auth.default_role` on an instance with `features.admin: false`.
-- Such an instance has no role endpoints, so it can only use the seeded roles,
-- whose permissions are locked. Its operator could only choose `member` or
-- nothing: open registration where every stranger can upload, or closed
-- registration where no one can join.
--
-- This row adds a third option: open registration where a new account can do
-- nothing until someone grants it more.
--
-- It also names a state that is currently implicit. An account in no role and
-- one deliberately held back look identical today, and only one of them is an
-- oversight.
--
-- No `role_permissions` rows; that is the point. `is_system` so it cannot be
-- deleted while a configuration file names it.

INSERT OR IGNORE INTO roles (id, name, description, is_system, created_at, updated_at)
VALUES ('00GEST00000000000000000000', 'guest', 'Signed in, and allowed nothing.', 1, '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z');
