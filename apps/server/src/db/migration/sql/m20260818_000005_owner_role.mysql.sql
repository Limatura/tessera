-- Splits `owner` out of `admin`.
--
-- # Why
--
-- The seeded set had `admin` (everything) and `member`, with nothing between
-- them. So an operator who wants somebody to help — see the accounts, hand out
-- invites, look at the shares — could only put them in `admin`, which also
-- hands over deleting accounts, destroying other people's content, and granting
-- permissions.
--
-- The three that separate them are exactly those:
--
--   users:delete       removing somebody
--   shares:delete:any  destroying content that is not yours
--   roles:write        handing out permissions, including these
--
-- `roles:write` is the one that matters most: without it an `admin` cannot
-- raise themselves, which is what makes the split hold rather than being a
-- suggestion.
--
-- # Why existing administrators are put in `owner`
--
-- Narrowing `admin` on an instance that already has people in it would take
-- powers away from whoever is administering it today — possibly including the
-- only person who could put them back. So everybody in `admin` is added to
-- `owner` first. Nobody loses anything, and `admin` means the same thing on
-- every instance from here on, which is the whole reason the seeded roles are
-- locked.
--
-- An operator will see those people in two roles. That is honest: they hold
-- what they held.

INSERT IGNORE INTO roles (id, name, description, is_system, created_at, updated_at)
VALUES ('00WNER00000000000000000000', 'owner', 'Everything, including removing accounts and content.', TRUE, '1970-01-01 00:00:00', '1970-01-01 00:00:00');

INSERT IGNORE INTO role_permissions (role_id, permission)
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
    ('00WNER00000000000000000000', 'stats:read');

-- Everybody who administers this instance today keeps administering it.
INSERT IGNORE INTO user_roles (user_id, role_id, granted_at)
SELECT user_id, '00WNER00000000000000000000', granted_at
  FROM user_roles
 WHERE role_id = '00ADMN00000000000000000000';

-- Only now is it safe to narrow `admin`.
DELETE FROM role_permissions
 WHERE role_id = '00ADMN00000000000000000000'
   AND permission IN ('users:delete', 'shares:delete:any', 'roles:write');

UPDATE roles
   SET description = 'Help run the instance: accounts, invites, and looking at what is here.'
 WHERE id = '00ADMN00000000000000000000';
