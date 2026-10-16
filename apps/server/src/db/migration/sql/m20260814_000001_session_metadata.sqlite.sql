-- Where a session started, so sessions in a list can be told apart.
--
-- # Why not "where it is now"
--
-- Both columns are written once, at session creation, and never updated. A
-- user lists sessions because they suspect one is not theirs; a row that
-- followed the latest request would show a stolen session with the thief's
-- latest address, or even the owner's own if they used it last. What
-- identifies a session is where it began.
--
-- # Why nullable
--
-- Sessions created before this column have no answer, and `NULL` says so. A
-- default of `''` or `'unknown'` would be indistinguishable from a session we
-- genuinely could not place, which is a real case: an instance behind an
-- undeclared proxy has no address worth recording.
--
-- # Why the user agent is stored as received
--
-- Turning it into "Chrome on macOS" is the display's job. Parsing these
-- strings is notoriously unreliable, and a wrong answer is worse than none: it
-- could tell a user a session is theirs when it is not. Length is capped in
-- the repository, since the client controls it.
ALTER TABLE sessions ADD COLUMN created_ip TEXT;

ALTER TABLE sessions ADD COLUMN created_user_agent TEXT;
