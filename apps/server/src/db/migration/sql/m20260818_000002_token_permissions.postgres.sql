-- What one API token may do.
--
-- # Why a table and not a column on `api_tokens`
--
-- The original request suggested a delimited column, which would work for one
-- token with three words. But `role_permissions` already has this exact shape,
-- and a second spelling of the same idea would mean two ways to ask "does this
-- hold `shares:create`": one an indexed lookup, the other a string search that
-- also matches `shares:create:all`.
--
-- # Why every existing token gets everything
--
-- Until now a token was its owner. Backup scripts are using them right now,
-- and a migration that narrowed them would break those scripts with an error
-- naming a permission their authors never heard of. So existing tokens keep
-- what they had, and the narrower default applies only to new tokens.
--
-- That is less generous than it sounds: a token's permissions are intersected
-- with its owner's at request time, so one of these still loses `users:delete`
-- as soon as its owner does.

CREATE TABLE api_token_permissions (
    token_id   TEXT NOT NULL REFERENCES api_tokens (id) ON DELETE CASCADE,
    -- A `tessera_core::Permission`, stored as its `resource:verb` spelling.
    permission TEXT NOT NULL,
    PRIMARY KEY (token_id, permission)
);

-- Read on every request that carries a token.
CREATE INDEX idx_api_token_permissions_token ON api_token_permissions (token_id);

INSERT INTO api_token_permissions (token_id, permission)
SELECT id, 'shares:create' FROM api_tokens
UNION ALL SELECT id, 'shares:read' FROM api_tokens
UNION ALL SELECT id, 'shares:delete' FROM api_tokens
UNION ALL SELECT id, 'tokens:manage' FROM api_tokens
UNION ALL SELECT id, 'users:read' FROM api_tokens
UNION ALL SELECT id, 'users:write' FROM api_tokens
UNION ALL SELECT id, 'users:delete' FROM api_tokens
UNION ALL SELECT id, 'invites:read' FROM api_tokens
UNION ALL SELECT id, 'invites:create' FROM api_tokens
UNION ALL SELECT id, 'invites:delete' FROM api_tokens
UNION ALL SELECT id, 'roles:read' FROM api_tokens
UNION ALL SELECT id, 'roles:write' FROM api_tokens
UNION ALL SELECT id, 'stats:read' FROM api_tokens;
