-- Postgres mirror of m20261006_000001_auth_flow_return.sqlite.sql.
ALTER TABLE auth_flows ADD COLUMN IF NOT EXISTS return_to TEXT;
