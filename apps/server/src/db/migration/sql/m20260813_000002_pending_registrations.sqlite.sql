-- Registrations waiting for their address to be confirmed.
--
-- # Why the account does not exist yet
--
-- Anyone can type anyone's address. If the row went straight into `users`,
-- registering with another person's address would squat it, occupy a username,
-- and, with open registration, let a stranger use storage tied to an inbox
-- they cannot read. Holding the registration here until the link is followed
-- prevents all three, and leaves no half-real account to write rules about.
--
-- # Why the username is not reserved
--
-- There is no unique index on `username`, by design. Reserving it would make
-- "register and never confirm" a free way to lock up every good name, with
-- nothing an operator could do. The name is checked for real when the link is
-- followed; the cost is that a user may finish registering and learn the name
-- was taken in the meantime: rare, annoying, and retryable.
--
-- The address *is* unique: one live link per address, so registering twice
-- replaces the first instead of leaving two working links in one mailbox.
--
-- Only the token's digest is stored, as everywhere else. The password is
-- already hashed here: it is carried through the wait instead of asked for
-- again, and must never be readable from this table either.
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
