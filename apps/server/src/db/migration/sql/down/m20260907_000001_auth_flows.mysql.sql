-- Back to what the OIDC migration created. The rows are ephemeral either way.
DROP TABLE IF EXISTS auth_flows;

CREATE TABLE IF NOT EXISTS oidc_flows (
    state       VARCHAR(255) NOT NULL PRIMARY KEY,
    nonce       TEXT NOT NULL,
    verifier    TEXT NOT NULL,
    purpose     TEXT NOT NULL,
    user_id     VARCHAR(64),
    CONSTRAINT fk_oidc_flows_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    invite_code TEXT,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);

CREATE INDEX idx_oidc_flows_expires ON oidc_flows (expires_at);
