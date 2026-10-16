-- Back to what the OIDC migration created, with that migration's own types.
-- The rows are ephemeral either way.
DROP TABLE IF EXISTS auth_flows;

CREATE TABLE IF NOT EXISTS oidc_flows (
    state       VARCHAR(191) NOT NULL PRIMARY KEY,
    nonce       TEXT         NOT NULL,
    verifier    TEXT         NOT NULL,
    purpose     VARCHAR(32)  NOT NULL,
    user_id     VARCHAR(191),
    invite_code TEXT,
    created_at  DATETIME(6)  NOT NULL,
    expires_at  DATETIME(6)  NOT NULL,
    CONSTRAINT fk_oidc_flows_user FOREIGN KEY (user_id)
        REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX idx_oidc_flows_expires ON oidc_flows (expires_at);
