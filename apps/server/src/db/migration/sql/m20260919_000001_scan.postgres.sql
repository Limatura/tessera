-- See the SQLite file for why these exist. Only the types differ.
ALTER TABLE file_shares ADD COLUMN scan_state VARCHAR(16);
ALTER TABLE file_shares ADD COLUMN scan_detail VARCHAR(191);
ALTER TABLE text_shares ADD COLUMN scan_state VARCHAR(16);
ALTER TABLE text_shares ADD COLUMN scan_detail VARCHAR(191);
CREATE INDEX file_shares_scan_state ON file_shares (scan_state);
