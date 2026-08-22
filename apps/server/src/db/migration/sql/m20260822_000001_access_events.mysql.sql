-- Access events: who fetched a share, and when.
--
-- # Why this is off by default
--
-- The counters (`view_count`, `copy_count`, `bytes_served`) need no switch —
-- an integer does not tell anybody where a visitor was. A row per access does:
-- it is an address and a browser fingerprint, kept in the operator's database,
-- about a person who was handed a link and never signed anything.
--
-- Anonymity is one of the reasons to run this software at all, so switching
-- that off is a product decision the operator makes, not a technical default
-- somebody inherits. `share.access_log.enabled` starts `false` and nothing is
-- written until it does not.
--
-- # Why a table and not columns on `shares`
--
-- There is no number of columns that holds "every access". The same reason
-- `file_shares` exists.
--
-- # Why the address is nullable
--
-- Behind a proxy this instance does not trust, there is no honest answer. A
-- row that says NULL is one that says so; one carrying the proxy's own address
-- would be a record of the wrong person.
--
-- # Why this dialect looks different
--
-- `VARCHAR(191)` rather than TEXT for anything an index touches: MySQL cannot
-- index a TEXT column without a prefix length, and 191 is the widest utf8mb4
-- column that fits the 767-byte limit older InnoDB builds still have.
--
-- The foreign key is table-level. `InnoDB` silently ignores a `REFERENCES`
-- written on a column, so the column form would create the table, pass every
-- test, and leave MySQL with no cascade — access events would outlive the share
-- they describe, which is the one row that must not be kept after the thing it
-- is about is gone.
--
-- `DATETIME(6)` rather than TIMESTAMP: TIMESTAMP stops at 2038 and errors
-- rather than truncating.
CREATE TABLE IF NOT EXISTS access_events (
    id           VARCHAR(191) NOT NULL PRIMARY KEY,
    share_id     VARCHAR(191) NOT NULL,
    occurred_at  DATETIME(6)  NOT NULL,
    -- What the request looked like. Both are what the caller presented.
    ip_address   VARCHAR(45),
    user_agent   TEXT,

    CONSTRAINT fk_access_events_share FOREIGN KEY (share_id) REFERENCES shares (id) ON DELETE CASCADE
);

-- The sweep reads by age and the listing reads by share; neither is the primary
-- key, and a table that grows one row per download needs both.
CREATE INDEX idx_access_events_occurred ON access_events (occurred_at);
CREATE INDEX idx_access_events_share ON access_events (share_id, occurred_at);
