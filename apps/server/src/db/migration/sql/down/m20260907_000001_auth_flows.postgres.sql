-- Back to what the OIDC migration created. The rows are ephemeral either way.
DROP TABLE IF EXISTS auth_flows;

CREATE TABLE IF NOT EXISTS oidc_flows (
    state       TEXT NOT NULL PRIMARY KEY,
    nonce       TEXT NOT NULL,
    verifier    TEXT NOT NULL,
    purpose     TEXT NOT NULL,
    user_id     TEXT REFERENCES users (id) ON DELETE CASCADE,
    invite_code TEXT,
    created_at  TIMESTAMPTZ NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_oidc_flows_expires ON oidc_flows (expires_at);
