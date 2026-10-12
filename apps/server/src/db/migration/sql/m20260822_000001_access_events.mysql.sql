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
--
-- # Why this dialect looks different
--
-- `VARCHAR(191)` instead of TEXT for anything indexed: MySQL cannot index a
-- TEXT column without a prefix length, and 191 is the widest utf8mb4 column
-- within the 767-byte limit of older InnoDB builds.
--
-- The foreign key is table-level. `InnoDB` silently ignores a `REFERENCES` on a
-- column, so the column form would create the table, pass every test, and
-- leave MySQL with no cascade: access events would outlive their share, which
-- is the one row that must not be kept once its subject is gone.
--
-- `DATETIME(6)` instead of TIMESTAMP: TIMESTAMP stops at 2038 and errors
-- instead of truncating.
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
