-- Splits `owner` out of `admin`.
--
-- # Why
--
-- The seeded set had `admin` (everything) and `member`, with nothing in
-- between. An operator wanting a helper (to see accounts, hand out invites,
-- look at shares) could only use `admin`, which also grants deleting
-- accounts, destroying other people's content, and granting permissions.
--
-- The three permissions that separate them are exactly those:
--
--   users:delete       removing an account
--   shares:delete:any  destroying content that is not yours
--   roles:write        granting permissions, including these
--
-- `roles:write` matters most: without it an `admin` cannot promote
-- themselves, so the split is enforced instead of advisory.
--
-- # Why existing administrators are put in `owner`
--
-- Narrowing `admin` on an instance already in use would take powers from
-- whoever administers it today, possibly including the only person who could
-- restore them. So everyone in `admin` is added to `owner` first. No one loses
-- anything, and `admin` means the same on every instance from here on, which
-- is why the seeded roles are locked.
--
-- An operator will see those accounts in two roles. That is accurate: they
-- hold what they held.

INSERT INTO roles (id, name, description, is_system, created_at, updated_at)
VALUES ('00WNER00000000000000000000', 'owner', 'Everything, including removing accounts and content.', TRUE, '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z')
ON CONFLICT DO NOTHING;

INSERT INTO role_permissions (role_id, permission)
VALUES
    ('00WNER00000000000000000000', 'shares:create'),
    ('00WNER00000000000000000000', 'shares:read'),
    ('00WNER00000000000000000000', 'shares:delete'),
    ('00WNER00000000000000000000', 'tokens:manage'),
    ('00WNER00000000000000000000', 'shares:read:any'),
    ('00WNER00000000000000000000', 'shares:delete:any'),
    ('00WNER00000000000000000000', 'users:read'),
    ('00WNER00000000000000000000', 'users:write'),
    ('00WNER00000000000000000000', 'users:delete'),
    ('00WNER00000000000000000000', 'invites:read'),
    ('00WNER00000000000000000000', 'invites:create'),
    ('00WNER00000000000000000000', 'invites:delete'),
    ('00WNER00000000000000000000', 'roles:read'),
    ('00WNER00000000000000000000', 'roles:write'),
    ('00WNER00000000000000000000', 'stats:read')
ON CONFLICT DO NOTHING;

-- Everybody who administers this instance today keeps administering it.
INSERT INTO user_roles (user_id, role_id, granted_at)
SELECT user_id, '00WNER00000000000000000000', granted_at
  FROM user_roles
 WHERE role_id = '00ADMN00000000000000000000'
ON CONFLICT DO NOTHING;

-- Only now is it safe to narrow `admin`.
DELETE FROM role_permissions
 WHERE role_id = '00ADMN00000000000000000000'
   AND permission IN ('users:delete', 'shares:delete:any', 'roles:write');

UPDATE roles
   SET description = 'Help run the instance: accounts, invites, and looking at what is here.'
 WHERE id = '00ADMN00000000000000000000';
