-- Invite codes, for an instance that is neither open nor closed.
--
-- # Why a code has a number of places on it
--
-- The case this exists for is a person letting their family or a group of
-- friends in. One code with five places is one thing to send to a group chat;
-- five codes is five things to hand out individually and then keep track of.
-- A single-use code is just this with `max_uses` of one.
--
-- # Why only the digest is stored
--
-- The same rule as sessions, API tokens and reset links: a stolen database must
-- not contain anything that can be presented as a credential. The code exists in
-- readable form once, on the terminal that made it.
--
-- # Why `used` is a count and not a list
--
-- Who used an invite is answerable from `users.created_at` and the invite it was
-- created against, if that is ever wanted. Keeping a list here would mean this
-- table holds a record of who knows whom, which is more than it needs to do its
-- job.
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

-- Which invite a waiting registration came in on.
--
-- The place is not taken until the account exists. Spending it when the form is
-- submitted would let somebody burn every place on a code without creating a
-- single account, and there is nothing an operator could do about that but issue
-- another code. So the invite is remembered here and charged at confirmation.
--
-- No foreign key: an operator deleting a spent invite should not take somebody's
-- half-finished registration with it, and by the time this is read the only
-- question is whether that invite still has room.
ALTER TABLE pending_registrations ADD COLUMN invite_id TEXT;
