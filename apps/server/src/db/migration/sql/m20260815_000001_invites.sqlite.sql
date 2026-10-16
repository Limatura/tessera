-- Invite codes, for an instance that is neither open nor closed.
--
-- # Why a code has several places
--
-- The main case is letting in family or a group of friends. One code with five
-- places is one thing to post in a group chat; five codes are five things to
-- hand out and track. A single-use code is this with `max_uses` of one.
--
-- # Why only the digest is stored
--
-- Same rule as sessions, API tokens and reset links: a stolen database must
-- contain nothing usable as a credential. The readable code exists once, on
-- the terminal that created it.
--
-- # Why `used` is a count, not a list
--
-- Who used an invite can be answered from `users.created_at` and the invite the
-- account was created against, if ever needed. A list here would make this
-- table a record of who knows whom, more than its job requires.
CREATE TABLE IF NOT EXISTS invites (
    id          TEXT    NOT NULL PRIMARY KEY,
    code_hash   TEXT    NOT NULL UNIQUE,
    note        TEXT,
    max_uses    INTEGER NOT NULL,
    used        INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT    NOT NULL,
    expires_at  TEXT
);

-- The sweep scans by this, and it is nullable because an invite that never
-- expires is a reasonable thing for one person's instance to have.
CREATE INDEX IF NOT EXISTS idx_invites_expires_at ON invites (expires_at);

-- Which invite a pending registration came in on.
--
-- The place is not consumed until the account exists. Consuming it on form
-- submission would let anyone burn every place on a code without creating a
-- single account, and the operator's only remedy would be issuing another
-- code. So the invite is remembered here and consumed at confirmation.
--
-- No foreign key: an operator deleting a used-up invite should not take a
-- half-finished registration with it, and by the time this is read the only
-- question is whether that invite still has room.
ALTER TABLE pending_registrations ADD COLUMN invite_id TEXT;
