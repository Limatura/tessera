-- Establishes the migration chain. Domain tables (shares, files, retrieval
-- codes) land in later migrations; this one only records deployment-level
-- metadata so there is always a schema to migrate *from*.
CREATE TABLE IF NOT EXISTS schema_metadata (
    key        TEXT    NOT NULL PRIMARY KEY,
    value      TEXT    NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
