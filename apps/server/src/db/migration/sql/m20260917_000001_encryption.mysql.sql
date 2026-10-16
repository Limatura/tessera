-- See the SQLite file for why these exist. Only the types differ.
ALTER TABLE shares ADD COLUMN encryption VARCHAR(16);
ALTER TABLE shares ADD COLUMN key_salt VARCHAR(32);
ALTER TABLE shares ADD COLUMN sealed_details TEXT;
ALTER TABLE file_shares ADD COLUMN sealed_name TEXT;
ALTER TABLE uploads ADD COLUMN encryption VARCHAR(16);
ALTER TABLE uploads ADD COLUMN key_salt VARCHAR(32);
ALTER TABLE uploads ADD COLUMN sealed_name TEXT;
