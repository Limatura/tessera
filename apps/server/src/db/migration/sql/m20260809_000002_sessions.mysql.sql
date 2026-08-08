-- MySQL/MariaDB mirror of m20260809_000002_sessions.sqlite.sql.
--
-- Fixed widths rather than TEXT: both `id` and `secret_hash` are indexed, and
-- MySQL cannot index TEXT without a prefix length. A ULID is 26 characters and a
-- SHA-256 digest in hex is 64, so both are exact.
CREATE TABLE IF NOT EXISTS sessions (
    id           CHAR(26)  NOT NULL PRIMARY KEY,
    user_id      CHAR(26)  NOT NULL,
    secret_hash  CHAR(64)  NOT NULL UNIQUE,
    created_at   TIMESTAMP NOT NULL,
    last_seen_at TIMESTAMP NOT NULL,
    expires_at   TIMESTAMP NOT NULL,
    CONSTRAINT fk_sessions_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_sessions_user ON sessions (user_id);

CREATE INDEX idx_sessions_expires_at ON sessions (expires_at);
