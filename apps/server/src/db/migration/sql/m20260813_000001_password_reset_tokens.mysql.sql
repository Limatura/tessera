-- MySQL/MariaDB mirror of m20260813_000001_password_reset_tokens.sqlite.sql.
CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id          CHAR(26)     NOT NULL PRIMARY KEY,
    user_id     CHAR(26)     NOT NULL,
    secret_hash CHAR(64)     NOT NULL UNIQUE,
    created_at  TIMESTAMP(6) NOT NULL,
    expires_at  TIMESTAMP(6) NOT NULL,
    used_at     TIMESTAMP(6) NULL,
    CONSTRAINT fk_password_reset_tokens_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_password_reset_tokens_user ON password_reset_tokens (user_id);
CREATE INDEX idx_password_reset_tokens_expires_at ON password_reset_tokens (expires_at);
