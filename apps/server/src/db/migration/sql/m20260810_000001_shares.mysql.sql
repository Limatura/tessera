-- MySQL/MariaDB mirror of m20260810_000001_shares.sqlite.sql.
--
-- `MEDIUMBLOB` and not `BLOB`: MySQL's `BLOB` tops out at 65,535 bytes, one byte
-- short of the 64 KiB inline threshold. The larger type costs nothing for rows
-- that do not use it and removes a truncation that would only appear on one
-- backend, at the boundary, for the largest inline share anyone happens to make.
CREATE TABLE IF NOT EXISTS shares (
    id             CHAR(26)     NOT NULL PRIMARY KEY,
    code           VARCHAR(64)  NOT NULL UNIQUE,
    user_id        CHAR(26)     NULL DEFAULT NULL,
    kind           VARCHAR(16)  NOT NULL,

    password_hash  VARCHAR(191) NULL DEFAULT NULL,
    use_limit      INT          NULL DEFAULT NULL,
    use_count      INT          NOT NULL DEFAULT 0,
    expires_at     TIMESTAMP(6) NULL DEFAULT NULL,
    disabled_at    TIMESTAMP(6) NULL DEFAULT NULL,

    title          VARCHAR(191) NULL DEFAULT NULL,
    filename       VARCHAR(191) NULL DEFAULT NULL,
    content_type   VARCHAR(191) NULL DEFAULT NULL,
    language       VARCHAR(64)  NULL DEFAULT NULL,
    size_bytes     BIGINT       NOT NULL,

    storage        VARCHAR(16)  NOT NULL,
    object_key     VARCHAR(191) NULL DEFAULT NULL,

    og_config      TEXT         NULL DEFAULT NULL,
    management_token_hash CHAR(64) NULL DEFAULT NULL,
    view_count     INT          NOT NULL DEFAULT 0,
    last_accessed_at TIMESTAMP(6) NULL DEFAULT NULL,

    created_at     TIMESTAMP(6) NOT NULL,
    updated_at     TIMESTAMP(6) NOT NULL,
    CONSTRAINT fk_shares_user FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE SET NULL
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;

CREATE INDEX idx_shares_user ON shares (user_id);
CREATE INDEX idx_shares_expires_at ON shares (expires_at);

CREATE TABLE IF NOT EXISTS share_contents (
    share_id CHAR(26)   NOT NULL PRIMARY KEY,
    content  MEDIUMBLOB NOT NULL,
    CONSTRAINT fk_share_contents_share FOREIGN KEY (share_id) REFERENCES shares (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARSET = utf8mb4;
