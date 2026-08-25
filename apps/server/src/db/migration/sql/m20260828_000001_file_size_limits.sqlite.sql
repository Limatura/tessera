-- Per-role and per-account limits on the size of a single file.
--
-- `server.max_body_bytes` is the only one today, it is instance-wide, and it is
-- read by axum's body-limit layer when the router is built — so it is fixed at
-- boot and it applies to everybody equally. An operator who raises it so they
-- can send a large file has also raised it for every stranger who can reach the
-- instance, and on a pool of free-tier buckets one stranger's upload is a
-- meaningful fraction of everything they have.
--
-- Same three-way reading as `quota_bytes`, and for the same reason: NULL is
-- "not set here, ask the next layer down", zero is "no limit at this layer".
-- The two are opposite instructions, so neither column can be NOT NULL.
--
-- `server.max_body_bytes` still applies on top of both. Nothing here can raise
-- it, because the layer that enforces it never sees these.
ALTER TABLE roles ADD COLUMN max_file_bytes INTEGER;
ALTER TABLE users ADD COLUMN max_file_bytes INTEGER;
