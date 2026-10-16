-- MySQL/MariaDB mirror of m20261013_000001_mfa.sqlite.sql.
CREATE TABLE IF NOT EXISTS totp_authenticators (
    user_id        CHAR(26)    NOT NULL PRIMARY KEY,
    secret         TEXT        NOT NULL,
    confirmed_at   DATETIME(6) NULL,
    last_used_step BIGINT      NULL,
    created_at     DATETIME(6) NOT NULL,
    CONSTRAINT fk_totp_authenticators_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE TABLE IF NOT EXISTS recovery_codes (
    id         CHAR(26)    NOT NULL PRIMARY KEY,
    user_id    CHAR(26)    NOT NULL,
    code_hash  CHAR(64)    NOT NULL,
    created_at DATETIME(6) NOT NULL,
    used_at    DATETIME(6) NULL,
    CONSTRAINT fk_recovery_codes_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_recovery_codes_user ON recovery_codes (user_id);

CREATE TABLE IF NOT EXISTS mfa_challenges (
    secret_hash CHAR(64)    NOT NULL PRIMARY KEY,
    user_id     CHAR(26)    NOT NULL,
    created_at  DATETIME(6) NOT NULL,
    expires_at  DATETIME(6) NOT NULL,
    CONSTRAINT fk_mfa_challenges_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_mfa_challenges_expires_at ON mfa_challenges (expires_at);
