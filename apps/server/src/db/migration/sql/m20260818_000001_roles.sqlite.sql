-- Roles, replacing the `is_admin` boolean.
--
-- # Why now
--
-- OIDC brings group claims, and a boolean cannot express "this group may upload
-- but not delete". Doing it after OIDC means revisiting every handler; doing it
-- now means changing one extractor, because that is all `is_admin` is read
-- through today.
--
-- # Why roles are rows and permissions are not
--
-- A permission exists only if some check names it, so the set of them is
-- decided by the code — see `tessera_core::Permission`. Which permissions
-- belong together is the part each instance decides, and the useful groupings
-- are not the ones anybody designs up front. So roles are rows and their
-- contents are rows, and the vocabulary they are written in is not.
--
-- # Why `is_system`
--
-- An instance whose only administrator role has been deleted has nobody who
-- can create one. The two seeded roles are marked so the endpoints can refuse
-- to remove them. It is not a claim that they cannot be edited — an operator
-- who wants `member` to stop being able to delete shares should be able to say
-- so.

CREATE TABLE roles (
    id          TEXT NOT NULL PRIMARY KEY,
    -- What an operator types and reads. Unique, because a role is referred to
    -- by name in every conversation about one.
    name        TEXT NOT NULL,
    description TEXT,
    -- Whether removing it would leave the instance unadministrable.
    is_system   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_roles_name ON roles (name);

CREATE TABLE role_permissions (
    role_id    TEXT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    -- A `tessera_core::Permission`, stored as its `resource:verb` spelling.
    permission TEXT NOT NULL,
    PRIMARY KEY (role_id, permission)
);

CREATE TABLE user_roles (
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role_id    TEXT NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    granted_at TEXT NOT NULL,
    PRIMARY KEY (user_id, role_id)
);

-- Indexed the way it is read: every authenticated request asks which roles one
-- user is in.
CREATE INDEX idx_user_roles_user ON user_roles (user_id);
CREATE INDEX idx_user_roles_role ON user_roles (role_id);

-- The two roles the boolean used to mean. Fixed identifiers rather than
-- generated ones so the three dialects seed the same rows and so the rest of
-- the program can name them without a lookup. Spelled in Crockford base32 like
-- every other identifier here, which is why `admin` reads as `ADMN` — that
-- alphabet has no `I`.
INSERT INTO roles (id, name, description, is_system, created_at, updated_at)
VALUES
    ('00ADMN00000000000000000000', 'admin', 'Everything.', 1, '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z'),
    ('00MEMBER000000000000000000', 'member', 'Upload, and manage what you uploaded.', 1, '1970-01-01T00:00:00Z', '1970-01-01T00:00:00Z');

INSERT INTO role_permissions (role_id, permission)
VALUES
    ('00ADMN00000000000000000000', 'shares:create'),
    ('00ADMN00000000000000000000', 'shares:read'),
    ('00ADMN00000000000000000000', 'shares:delete'),
    ('00ADMN00000000000000000000', 'tokens:manage'),
    ('00ADMN00000000000000000000', 'users:read'),
    ('00ADMN00000000000000000000', 'users:write'),
    ('00ADMN00000000000000000000', 'users:delete'),
    ('00ADMN00000000000000000000', 'invites:read'),
    ('00ADMN00000000000000000000', 'invites:create'),
    ('00ADMN00000000000000000000', 'invites:delete'),
    ('00ADMN00000000000000000000', 'roles:read'),
    ('00ADMN00000000000000000000', 'roles:write'),
    ('00ADMN00000000000000000000', 'stats:read'),
    ('00MEMBER000000000000000000', 'shares:create'),
    ('00MEMBER000000000000000000', 'shares:read'),
    ('00MEMBER000000000000000000', 'shares:delete'),
    ('00MEMBER000000000000000000', 'tokens:manage');

-- Everybody keeps exactly what they had. An administrator becomes an
-- administrator; everybody else becomes a member, which is the set of things a
-- non-administrator could already do.
INSERT INTO user_roles (user_id, role_id, granted_at)
SELECT id, '00ADMN00000000000000000000', created_at FROM users WHERE is_admin = 1;

INSERT INTO user_roles (user_id, role_id, granted_at)
SELECT id, '00MEMBER000000000000000000', created_at FROM users WHERE is_admin = 0;

-- `is_admin` goes in a later migration, not this one. Dropping it here would
-- mean this change can only be rolled forward: a server that has run this and
-- is then rolled back finds no column where it expects one, and refuses to
-- start with no way out. Leaving it costs one stale boolean for one release.
