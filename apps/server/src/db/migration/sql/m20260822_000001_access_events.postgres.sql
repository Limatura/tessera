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
CREATE TABLE IF NOT EXISTS access_events (
    id           TEXT NOT NULL PRIMARY KEY,
    share_id     TEXT NOT NULL REFERENCES shares (id) ON DELETE CASCADE,
    occurred_at  TIMESTAMPTZ NOT NULL,
    -- What the request looked like. Both are what the caller presented.
    ip_address   TEXT,
    user_agent   TEXT
);

-- The sweep reads by age and the listing reads by share; neither is the primary
-- key, and a table that grows one row per download needs both.
CREATE INDEX IF NOT EXISTS idx_access_events_occurred ON access_events (occurred_at);
CREATE INDEX IF NOT EXISTS idx_access_events_share ON access_events (share_id, occurred_at);
