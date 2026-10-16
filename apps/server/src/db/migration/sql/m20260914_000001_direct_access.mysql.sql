-- See the SQLite file for why these exist. Only the types differ.
ALTER TABLE storage_backends ADD COLUMN direct_access BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE shares ADD COLUMN bytes_redirected BIGINT NOT NULL DEFAULT 0;
