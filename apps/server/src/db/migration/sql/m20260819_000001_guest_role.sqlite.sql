-- A role that is allowed nothing.
--
-- # Why a role with no permissions is worth a row
--
-- Not for what it grants. Retrieval never looks at an account — a code, a
-- password and a ticket are the whole of it — so "may download but not upload"
-- already describes somebody with no account at all, and needs no role.
--
-- What it is for is `auth.default_role` on an instance running with
-- `features.admin: false`. That instance has no role endpoints, so it can only
-- use the seeded roles, and their permissions are locked. Its operator could
-- therefore choose between `member` and nothing at all: open registration and
-- every stranger can upload, or closed registration and nobody can join.
--
-- With this row there is a third answer — open registration, where a new
-- account can do nothing until somebody says otherwise.
--
-- It also gives a name to a state that is currently silent. An account in no
-- role and an account deliberately held back look identical today, and only one
-- of them is somebody forgetting.
--
-- No `role_permissions` rows, which is the whole point. `is_system` so it
-- cannot be deleted out from under a configuration file naming it.

INSERT OR IGNORE INTO roles (id, name, description, is_system, created_at, updated_at)
VALUES ('00GEST00000000000000000000', 'guest', 'Signed in, and allowed nothing.', 1, '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z');
