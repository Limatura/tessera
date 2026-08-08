-- MySQL/MariaDB mirror of m20260809_000003_api_tokens.sqlite.sql.
CREATE TABLE IF NOT EXISTS api_tokens (
    id             CHAR(26)     NOT NULL PRIMARY KEY,
    user_id        CHAR(26)     NOT NULL,
    name           VARCHAR(191) NOT NULL,
    secret_hash    CHAR(64)     NOT NULL UNIQUE,
    display_prefix VARCHAR(32)  NOT NULL,
    created_at     TIMESTAMP    NOT NULL,
    last_used_at   TIMESTAMP    NULL DEFAULT NULL,
    expires_at     TIMESTAMP    NULL DEFAULT NULL,
    revoked_at     TIMESTAMP    NULL DEFAULT NULL,
    CONSTRAINT fk_api_tokens_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_api_tokens_user ON api_tokens (user_id);
