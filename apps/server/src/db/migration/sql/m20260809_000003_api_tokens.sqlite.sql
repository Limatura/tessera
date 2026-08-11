-- API tokens, for scripts, the CLI, ShareX and the Discord bot.
--
-- Separate from sessions on purpose: an automation credential is long-lived,
-- pasted into a config file and revoked individually, where a browser session is
-- short-lived, invisible to JavaScript and dropped on logout. One table serving
-- both would end up too weak for the first job or too awkward for the second.
--
-- `display_prefix` holds the first few characters of the token so a listing can
-- say which one a row is without revealing it — the value itself is never
-- recoverable after creation.
CREATE TABLE IF NOT EXISTS api_tokens (
    id             TEXT NOT NULL PRIMARY KEY,
    user_id        TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name           TEXT NOT NULL,
    secret_hash    TEXT NOT NULL UNIQUE,
    display_prefix TEXT NOT NULL,
    created_at     TEXT NOT NULL,
    last_used_at   TEXT,
    -- Both nullable: a token may live forever, and revocation is recorded rather
    -- than deleted so an audit still shows the token existed.
    expires_at     TEXT,
    revoked_at     TEXT
);

CREATE INDEX IF NOT EXISTS idx_api_tokens_user ON api_tokens (user_id);
