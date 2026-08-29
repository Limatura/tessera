-- Runtime settings, as changed from the admin panel.
--
-- # Why the database and not a file
--
-- The panel is the interface, and a file it writes back has three problems a
-- table does not: the environment still wins over it silently, a read-only
-- mount cannot be written at all, and rewriting YAML throws away every comment
-- in it. A row is atomic, and a half-written one cannot exist.
--
-- # Only what can be changed while running
--
-- `database.*`, `telemetry.*` and the listening address are not here and never
-- will be: reading this table needs the database open, the log subscriber is
-- installed once, and the socket is already bound. Those stay in the
-- environment. Everything in here is something the running process can pick up,
-- so the panel never has to say "saved, now restart" — if it is in this table
-- it takes effect.
--
-- # One row per setting, not one row of JSON
--
-- So that clearing a single setting is a DELETE, and two administrators
-- changing different settings do not overwrite each other.
--
-- # Why the column is not called `key`
--
-- It is reserved in MySQL. Quoting it everywhere works until somebody writes
-- one query without the backticks.
CREATE TABLE IF NOT EXISTS settings (
    name        TEXT NOT NULL PRIMARY KEY,
    -- The value as JSON, so that a number stays a number and the layering
    -- below it sees the same shape the file and the environment produce.
    value       TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
