-- MySQL/MariaDB mirror of m20260810_000002_share_tickets.sqlite.sql.
CREATE TABLE IF NOT EXISTS share_tickets (
    id          CHAR(26)     NOT NULL PRIMARY KEY,
    share_id    CHAR(26)     NOT NULL,
    secret_hash CHAR(64)     NOT NULL UNIQUE,
    created_at  TIMESTAMP(6) NOT NULL,
    expires_at  TIMESTAMP(6) NOT NULL,
    CONSTRAINT fk_share_tickets_share FOREIGN KEY (share_id) REFERENCES shares (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_share_tickets_share ON share_tickets (share_id);
CREATE INDEX idx_share_tickets_expires_at ON share_tickets (expires_at);
