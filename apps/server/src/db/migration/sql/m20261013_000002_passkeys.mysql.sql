-- MySQL/MariaDB mirror of m20261013_000002_passkeys.sqlite.sql.
CREATE TABLE IF NOT EXISTS passkeys (
    id              CHAR(26)    NOT NULL PRIMARY KEY,
    user_id         CHAR(26)    NOT NULL,
    credential_hash CHAR(64)    NOT NULL UNIQUE,
    credential_id   TEXT        NOT NULL,
    public_key      TEXT        NOT NULL,
    sign_count      BIGINT      NOT NULL,
    name            VARCHAR(64) NOT NULL,
    backup_eligible BOOLEAN     NOT NULL,
    backed_up       BOOLEAN     NOT NULL,
    created_at      DATETIME(6) NOT NULL,
    last_used_at    DATETIME(6) NULL,
    CONSTRAINT fk_passkeys_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_passkeys_user ON passkeys (user_id);

CREATE TABLE IF NOT EXISTS webauthn_challenges (
    secret_hash CHAR(64)    NOT NULL PRIMARY KEY,
    purpose     VARCHAR(16) NOT NULL,
    user_id     CHAR(26)    NULL,
    created_at  DATETIME(6) NOT NULL,
    expires_at  DATETIME(6) NOT NULL,
    CONSTRAINT fk_webauthn_challenges_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_webauthn_challenges_expires_at ON webauthn_challenges (expires_at);
