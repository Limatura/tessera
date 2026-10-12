-- Runtime settings, as changed from the admin panel.
--
-- # Why the database and not a file
--
-- The panel is the interface, and a file it writes back has three problems a
-- table does not: the environment still silently overrides it, a read-only
-- mount cannot be written, and rewriting YAML loses its comments. A row
-- write is atomic; there is no half-written state.
--
-- # Only what can change at runtime
--
-- `database.*`, `telemetry.*` and the listen address are not here and never
-- will be: reading this table needs the database open, the log subscriber is
-- installed once, and the socket is already bound. Those stay in the
-- environment. Everything here can be picked up by the running process, so
-- the panel never says "saved, now restart": if it is in this table it takes
-- effect.
--
-- # One row per setting, not one JSON row
--
-- So clearing one setting is a DELETE, and two administrators changing
-- different settings do not overwrite each other.
--
-- # Why the column is not called `key`
--
-- It is reserved in MySQL. Quoting it everywhere works until one query
-- forgets the backticks.
--
-- # Why this dialect looks different
--
-- `VARCHAR(191)` for the key: MySQL cannot make a TEXT column a primary key
-- without a prefix length, and 191 is the widest utf8mb4 column within the
-- 767-byte index limit of older InnoDB builds.
--
-- `DATETIME(6)` instead of TIMESTAMP, which stops at 2038 and errors.
CREATE TABLE IF NOT EXISTS settings (
    name        VARCHAR(191) NOT NULL PRIMARY KEY,
    -- The value as JSON, so that a number stays a number and the layering
    -- below it sees the same shape the file and the environment produce.
    value       TEXT         NOT NULL,
    updated_at  DATETIME(6)  NOT NULL
);
