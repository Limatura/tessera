-- Lets an administrator reach shares that are not theirs.
--
-- # Why this is a migration and not an edit to the roles one
--
-- Instances have already run `m20260818_000001_roles`. Editing its SQL would
-- change nothing on any of them and would only make the file lie about what
-- their database contains.
--
-- # Why the administrator role and not every role
--
-- These two reach other people's content, which is the line `is_privileged`
-- draws. A role somebody built for uploaders should not silently gain the
-- ability to delete a stranger's file because this version shipped.
--
-- `INSERT OR IGNORE` because a fresh instance runs every migration in order and
-- an operator may have added the rows by hand.

INSERT OR IGNORE INTO role_permissions (role_id, permission)
VALUES
    ('00ADMN00000000000000000000', 'shares:read:any'),
    ('00ADMN00000000000000000000', 'shares:delete:any');
