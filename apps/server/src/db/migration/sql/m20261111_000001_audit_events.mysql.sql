-- The audit log: who did what to which thing, and whether it worked.
--
-- See the SQLite file for why this is its own table, why it has no foreign
-- keys, and why `metadata` is text.
--
-- # Why this dialect looks different
--
-- `VARCHAR(191)` for anything indexed: MySQL cannot index TEXT without a
-- prefix length, and 191 is the widest utf8mb4 column within the 767-byte
-- limit of older InnoDB builds. `DATETIME(6)` because TIMESTAMP stops at 2038.
CREATE TABLE IF NOT EXISTS audit_events (
    id             VARCHAR(191) NOT NULL PRIMARY KEY,
    occurred_at    DATETIME(6)  NOT NULL,
    action         VARCHAR(64)  NOT NULL,
    outcome        VARCHAR(16)  NOT NULL,
    actor_kind     VARCHAR(16)  NOT NULL,
    actor_user_id  VARCHAR(191),
    api_token_id   VARCHAR(191),
    target_type    VARCHAR(32),
    target_id      VARCHAR(191),
    ip_address     VARCHAR(45),
    user_agent     TEXT,
    metadata       TEXT
);

CREATE INDEX idx_audit_events_occurred ON audit_events (occurred_at);
CREATE INDEX idx_audit_events_actor ON audit_events (actor_user_id, occurred_at);
CREATE INDEX idx_audit_events_target ON audit_events (target_type, target_id, occurred_at);
CREATE INDEX idx_audit_events_action ON audit_events (action, occurred_at);

INSERT IGNORE INTO role_permissions (role_id, permission)
VALUES
    ('00WNER00000000000000000000', 'audit:read'),
    ('00ADMN00000000000000000000', 'audit:read');
