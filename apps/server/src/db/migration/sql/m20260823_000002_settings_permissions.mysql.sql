-- The two settings permissions, granted to the roles that already run the
-- instance.
--
-- # Why owner gets write and admin does not
--
-- `admin` is the "help run the instance" role: accounts, invites, and looking
-- at what is here. Changing how the instance is configured is a different kind
-- of act — one bad value and uploads stop working for everybody — and it
-- belongs with the role that can already grant itself anything.
--
-- `admin` gets read, because answering "why is the upload limit 100 MB" is
-- exactly the question that role exists to field, and reading the list turns
-- nothing.
--
-- An operator who disagrees can move it: that is what the role endpoints are
-- for. This is the starting point, not a rule.
--
-- `INSERT OR IGNORE` / `ON CONFLICT DO NOTHING`, because an instance may have
-- been given these by hand already.
INSERT IGNORE INTO role_permissions (role_id, permission)
VALUES
    ('00WNER00000000000000000000', 'settings:read'),
    ('00WNER00000000000000000000', 'settings:write'),
    ('00ADMN00000000000000000000', 'settings:read');
