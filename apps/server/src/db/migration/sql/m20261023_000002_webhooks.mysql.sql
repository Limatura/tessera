-- MySQL/MariaDB mirror of m20261023_000002_webhooks.sqlite.sql.
CREATE TABLE IF NOT EXISTS webhooks (
    id         CHAR(26)    NOT NULL PRIMARY KEY,
    user_id    CHAR(26)    NULL,
    url        TEXT        NOT NULL,
    events     TEXT        NOT NULL,
    format     VARCHAR(16) NOT NULL,
    secret     TEXT        NOT NULL,
    created_at DATETIME(6) NOT NULL,
    CONSTRAINT fk_webhooks_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_webhooks_user ON webhooks (user_id);

CREATE TABLE IF NOT EXISTS webhook_deliveries (
    id              CHAR(26)    NOT NULL PRIMARY KEY,
    webhook_id      CHAR(26)    NOT NULL,
    event           VARCHAR(32) NOT NULL,
    body            MEDIUMTEXT  NOT NULL,
    attempts        INT         NOT NULL DEFAULT 0,
    next_attempt_at DATETIME(6) NULL,
    delivered_at    DATETIME(6) NULL,
    last_status     INT         NULL,
    last_error      TEXT        NULL,
    created_at      DATETIME(6) NOT NULL,
    CONSTRAINT fk_webhook_deliveries_webhook FOREIGN KEY (webhook_id) REFERENCES webhooks (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_webhook_deliveries_due ON webhook_deliveries (next_attempt_at);
CREATE INDEX idx_webhook_deliveries_webhook ON webhook_deliveries (webhook_id, created_at);

ALTER TABLE shares ADD COLUMN end_announced_at DATETIME(6) NULL;

CREATE INDEX idx_shares_end_announced ON shares (end_announced_at, expires_at);
