-- MySQL/MariaDB mirror of m20261006_000001_auth_flow_return.sqlite.sql.
ALTER TABLE auth_flows ADD COLUMN return_to VARCHAR(512) NULL;
