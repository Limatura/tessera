-- Registrations waiting for their address to be confirmed.
--
-- # Why the account does not exist yet
--
-- Anyone can type anyone's address. If the row went straight into `users`,
-- registering with somebody else's address would squat their address, occupy a
-- username, and — on an instance with open registration — let a stranger use
-- the storage behind an inbox they cannot read. Holding the registration here
-- until the link is followed removes all three at once, and there is no
-- half-real account to write rules about afterwards.
--
-- # Why the username is not reserved
--
-- There is no unique index on `username`, on purpose. Reserving it would make
-- "register and never confirm" a free way to lock every good name away, with
-- nothing an operator could do about it. The name is checked for real when the
-- link is followed; the cost is that somebody can complete a registration and
-- be told the name went to another person in the meantime, which is annoying
-- and rare and can be retried.
--
-- The address *is* unique: one live link per address, so registering twice
-- replaces the first rather than leaving two working links in one mailbox.
--
-- Only the digest of the token is stored, as everywhere else. The password is
-- already hashed here — it is carried through the wait rather than asked for
-- again, and it must never be readable from this table either.
CREATE TABLE IF NOT EXISTS pending_registrations (
    id            TEXT NOT NULL PRIMARY KEY,
    username      TEXT NOT NULL,
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    secret_hash   TEXT NOT NULL UNIQUE,
    created_at    TEXT NOT NULL,
    expires_at    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_pending_registrations_expires_at ON pending_registrations (expires_at);
