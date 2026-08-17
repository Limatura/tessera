-- What one API token may do.
--
-- # Why a table and not a column on `api_tokens`
--
-- The issue that asked for this said a delimited column would do, and for one
-- token holding three words it would. `role_permissions` already exists and has
-- exactly this shape, though, so a second spelling of the same idea would mean
-- two ways to ask "does this hold `shares:create`" — one an indexed lookup, the
-- other a string search that matches `shares:create:all` as well.
--
-- # Why every existing token is given everything
--
-- Today a token is its owner. Somebody's backup script is using one right now,
-- and a migration that narrows it would stop that script with an error naming a
-- permission its author has never heard of. So existing tokens keep exactly
-- what they had, and the narrower default applies only to tokens minted from
-- here on.
--
-- That is not as generous as it reads: what a token may do is intersected with
-- what its owner may do at the time of the request, so one of these still loses
-- `users:delete` the moment its owner does.

CREATE TABLE api_token_permissions (
    token_id   CHAR(26)     NOT NULL,
    -- A `tessera_core::Permission`, stored as its `resource:verb` spelling.
    permission VARCHAR(191) NOT NULL,
    PRIMARY KEY (token_id, permission),
    FOREIGN KEY (token_id) REFERENCES api_tokens (id) ON DELETE CASCADE
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
