-- `logs:read`, granted to the two roles that run the instance.
--
-- Both, unlike `settings:write`: reading the log changes nothing, and "why did
-- that upload fail" is the question the admin role exists to answer.
--
-- Already granted by hand is not an error, hence the conflict clause.
INSERT INTO role_permissions (role_id, permission)
VALUES
    ('00WNER00000000000000000000', 'logs:read'),
    ('00ADMN00000000000000000000', 'logs:read')
ON CONFLICT DO NOTHING;
