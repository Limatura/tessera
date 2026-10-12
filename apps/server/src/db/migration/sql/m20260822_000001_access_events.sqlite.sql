-- Access events: who fetched a share, and when.
--
-- # Why this is off by default
--
-- The counters (`view_count`, `copy_count`, `bytes_served`) need no switch:
-- an integer reveals nothing about where a visitor was. A row per access does.
-- It is an address and a browser fingerprint, stored in the operator's
-- database, about a person who was sent a link and agreed to nothing.
--
-- Anonymity is one of the reasons to run this software, so giving it up is a
-- product decision for the operator, not a technical default they inherit.
-- `share.access_log.enabled` starts `false` and nothing is written until it is
-- turned on.
--
-- # Why a table and not columns on `shares`
--
-- No number of columns can hold "every access", for the same reason
-- `file_shares` exists.
--
-- # Why the address is nullable
--
-- Behind a proxy this instance does not trust, there is no correct answer. A
-- NULL says so; the proxy's own address would record the wrong party.
CREATE TABLE IF NOT EXISTS access_events (
    id           TEXT NOT NULL PRIMARY KEY,
    share_id     TEXT NOT NULL REFERENCES shares (id) ON DELETE CASCADE,
    occurred_at  TEXT NOT NULL,
    -- What the request looked like. Both are what the caller presented.
    ip_address   TEXT,
    user_agent   TEXT
);

-- The sweep reads by age and the listing reads by share; neither is the primary
-- key, and a table that grows one row per download needs both.
CREATE INDEX IF NOT EXISTS idx_access_events_occurred ON access_events (occurred_at);
CREATE INDEX IF NOT EXISTS idx_access_events_share ON access_events (share_id, occurred_at);
