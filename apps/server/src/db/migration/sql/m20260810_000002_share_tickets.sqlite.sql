-- Download authorisations.
--
-- # Why this is a table and not a signed value
--
-- A ticket must survive a restart. It is issued when a share spends one of its
-- retrievals, then presented on every request the transfer needs: each
-- `Range`, each resumption, each retry after a dropped connection. If a
-- restart invalidated it, an interrupted download would have to re-authorise
-- and spend *another* retrieval; on a one-time share the payload would be gone
-- and the sender would have to upload it again.
--
-- A signed token survives a restart only if the signing key does, which means
-- a key to configure, a key to lose, and no way to revoke a ticket once issued.
-- A row costs a sweep and gives both.
--
-- Only the digest is stored, as with sessions and API tokens: a stolen
-- database contains nothing that can be presented.
CREATE TABLE IF NOT EXISTS share_tickets (
    id          TEXT NOT NULL PRIMARY KEY,
    share_id    TEXT NOT NULL REFERENCES shares (id) ON DELETE CASCADE,
    secret_hash TEXT NOT NULL UNIQUE,
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_share_tickets_share ON share_tickets (share_id);
CREATE INDEX IF NOT EXISTS idx_share_tickets_expires_at ON share_tickets (expires_at);
