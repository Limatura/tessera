-- MySQL/MariaDB mirror of m20260926_000001_email_verifications.sqlite.sql.
CREATE TABLE IF NOT EXISTS email_verifications (
    id          CHAR(26)     NOT NULL PRIMARY KEY,
    user_id     CHAR(26)     NOT NULL,
    email       VARCHAR(191) NOT NULL,
    secret_hash CHAR(64)     NOT NULL UNIQUE,
    created_at  DATETIME(6)  NOT NULL,
    expires_at  DATETIME(6)  NOT NULL,
    used_at     DATETIME(6)  NULL,
    CONSTRAINT fk_email_verifications_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_email_verifications_user ON email_verifications (user_id);
CREATE INDEX idx_email_verifications_expires_at ON email_verifications (expires_at);
