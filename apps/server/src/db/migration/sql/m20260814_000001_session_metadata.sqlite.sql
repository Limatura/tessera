-- Where a session started, so a list of them can be told apart.
--
-- # Why this is not "where it is now"
--
-- Both columns are written once, when the session is created, and never
-- updated. The point of listing sessions is that somebody suspects one of them
-- is not theirs — and a row that follows the current request would show a
-- stolen session wearing the thief's latest address, or worse, the owner's own
-- if they happened to use it last. What identifies a session is where it began.
--
-- # Why nullable
--
-- Sessions that existed before this column did have no answer, and `NULL` says
-- exactly that. A default of `''` or `'unknown'` would be indistinguishable
-- from a session we genuinely could not place — which is a real case, since an
-- instance reached through an undeclared proxy has no address worth recording.
--
-- # Why the user agent is stored as it arrived
--
-- Turning it into "Chrome on macOS" is a job for whatever draws the list.
-- Parsing these strings is famously unreliable, and a wrong answer here is
-- worse than no answer: it would tell somebody a session is theirs when it is
-- not. Length is capped in the repository, since the caller chooses it.
ALTER TABLE sessions ADD COLUMN created_ip TEXT;

ALTER TABLE sessions ADD COLUMN created_user_agent TEXT;
