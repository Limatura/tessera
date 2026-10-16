-- Accounts get a lifecycle instead of simply existing or not.
--
-- # Why three states and not a `disabled` boolean
--
-- Suspending and removing an account are different actions with different
-- consequences, and an operator must be able to tell later which happened. A
-- boolean can hold "suspended"; it cannot hold "this account was deleted on
-- the 3rd", the question asked months later.
--
-- # Why words and not 0/1/2
--
-- `kind` and `storage` on `shares` are already words, for the same reason:
-- someone reading the table in `psql` should not need this file open to know
-- what `2` meant.
--
-- # Why the row survives a deletion
--
-- `shares.user_id` is `ON DELETE SET NULL`, so removing the row would make
-- every share the account uploaded indistinguishable from an anonymous upload,
-- and "no one ever owned this" is a different fact from "the owner is gone".
-- Once merged they cannot be separated again.
--
-- Keeping the row also keeps `deleted_by` pointing at someone. The username
-- and address are scrubbed on deletion, which frees them for reuse while
-- leaving nothing personal behind.

ALTER TABLE users ADD COLUMN state TEXT NOT NULL DEFAULT 'active';

-- When the account was removed. Null for every account that has not been.
ALTER TABLE users ADD COLUMN deleted_at TIMESTAMPTZ;

-- Who removed it, or the account itself for a self-deletion.
--
-- References `users`, which works because deletion leaves the row behind: an
-- administrator who is later deleted still has a row to point at.
ALTER TABLE users ADD COLUMN deleted_by TEXT REFERENCES users (id) ON DELETE SET NULL;

-- Read by the listing, which filters deleted accounts out by default.
CREATE INDEX idx_users_state ON users (state);
