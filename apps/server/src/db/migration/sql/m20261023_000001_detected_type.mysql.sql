-- See the SQLite file for why this exists. Only the type differs.
ALTER TABLE file_shares ADD COLUMN detected_type VARCHAR(16);
