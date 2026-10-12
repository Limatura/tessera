-- Per-role and per-account limits on the size of a single file.
--
-- `server.max_body_bytes` is currently the only limit. It is instance-wide and
-- read by axum's body-limit layer when the router is built, so it is fixed at
-- boot and applies to everyone. An operator who raises it to send a large file
-- raises it for every stranger who can reach the instance too, and on a pool
-- of free-tier buckets one stranger's upload is a meaningful share of the
-- total space.
--
-- Same three-way reading as `quota_bytes`, for the same reason: NULL is "not
-- set here, ask the next layer", zero is "no limit at this layer". They are
-- opposite instructions, so neither column can be NOT NULL.
--
-- `server.max_body_bytes` still applies on top of both. Nothing here can raise
-- it, since the layer enforcing it never sees these.
ALTER TABLE roles ADD COLUMN max_file_bytes BIGINT;
ALTER TABLE users ADD COLUMN max_file_bytes BIGINT;
