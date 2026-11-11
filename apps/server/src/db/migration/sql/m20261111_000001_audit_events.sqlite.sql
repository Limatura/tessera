-- The audit log: who did what to which thing, and whether it worked.
--
-- # Why this is not `access_events`
--
-- `access_events` answers "who opened this one share", is off by default, and
-- dies with the share. Most of what is recorded here has no share at all (a
-- sign-in, a setting), and the record of an administrator removing something
-- must outlive the thing removed. So: a table of its own, and no foreign keys.
-- An entry naming an account or a share that no longer exists is the point.
--
-- # Why `metadata` is text
--
-- It is JSON, written and read only by this server. A JSON column type exists
-- on two of the three backends and means something different on each; text
-- means the same thing everywhere, and nothing here queries inside it.
--
-- # What is never in it
--
-- Secrets: a setting marked sealed is written as "changed", a token as its id,
-- and a failed sign-in never carries the address that was typed, so the log
-- cannot be read to learn which accounts exist.
--
-- # Why `audit:read` is granted here
--
-- To the owner and admin roles, the two that run the instance, the same as
-- `logs:read`. Already granted by hand is not an error.
CREATE TABLE IF NOT EXISTS audit_events (
    id             TEXT NOT NULL PRIMARY KEY,
    occurred_at    TEXT NOT NULL,
    action         TEXT NOT NULL,
    outcome        TEXT NOT NULL,
    actor_kind     TEXT NOT NULL,
    actor_user_id  TEXT,
    api_token_id   TEXT,
    target_type    TEXT,
    target_id      TEXT,
    ip_address     TEXT,
    user_agent     TEXT,
    metadata       TEXT
);

CREATE INDEX IF NOT EXISTS idx_audit_events_occurred ON audit_events (occurred_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_actor ON audit_events (actor_user_id, occurred_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_target ON audit_events (target_type, target_id, occurred_at);
CREATE INDEX IF NOT EXISTS idx_audit_events_action ON audit_events (action, occurred_at);

INSERT OR IGNORE INTO role_permissions (role_id, permission)
VALUES
    ('00WNER00000000000000000000', 'audit:read'),
    ('00ADMN00000000000000000000', 'audit:read');
