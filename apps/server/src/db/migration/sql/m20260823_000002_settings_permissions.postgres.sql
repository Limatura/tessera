-- The two settings permissions, granted to the roles that already run the
-- instance.
--
-- # Why owner gets write and admin does not
--
-- `admin` is the "help run the instance" role: accounts, invites, and viewing
-- what is here. Changing the instance's configuration is a different kind of
-- action (one bad value and uploads stop working for everyone), so it belongs
-- with the role that can already grant itself anything.
--
-- `admin` gets read, because "why is the upload limit 100 MB" is exactly the
-- question that role exists to answer, and reading the list changes nothing.
--
-- An operator who disagrees can change it with the role endpoints. This is a
-- starting point, not a rule.
--
-- `INSERT OR IGNORE` / `ON CONFLICT DO NOTHING`, because an instance may
-- already have been granted these by hand.
INSERT INTO role_permissions (role_id, permission)
VALUES
    ('00WNER00000000000000000000', 'settings:read'),
    ('00WNER00000000000000000000', 'settings:write'),
    ('00ADMN00000000000000000000', 'settings:read')
ON CONFLICT DO NOTHING;
