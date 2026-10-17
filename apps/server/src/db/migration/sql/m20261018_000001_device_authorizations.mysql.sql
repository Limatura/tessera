-- MySQL/MariaDB mirror of m20261018_000001_device_authorizations.sqlite.sql.
CREATE TABLE IF NOT EXISTS device_authorizations (
    device_code_hash CHAR(64)     NOT NULL PRIMARY KEY,
    user_code        CHAR(8)      NOT NULL UNIQUE,
    client_name      VARCHAR(191) NOT NULL,
    requested_from   VARCHAR(191) NOT NULL,
    user_id          CHAR(26)     NULL,
    permissions      TEXT         NULL,
    approved_at      DATETIME(6)  NULL,
    denied_at        DATETIME(6)  NULL,
    created_at       DATETIME(6)  NOT NULL,
    expires_at       DATETIME(6)  NOT NULL,
    CONSTRAINT fk_device_authorizations_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_device_authorizations_expires_at ON device_authorizations (expires_at);
