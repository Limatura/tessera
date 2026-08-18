-- Accounts get a lifecycle instead of existing or not.
--
-- # Why three states and not a `disabled` boolean
--
-- Suspending somebody and removing them are different acts with different
-- consequences, and an operator has to be able to tell afterwards which one
-- happened. A boolean can hold "suspended"; it cannot hold "this account was
-- deleted on the 3rd", which is the fact somebody asks about months later.
--
-- # Why words and not 0/1/2
--
-- `kind` and `storage` on `shares` are already words, and the reason is the
-- same: somebody reading the table in `psql` should not need this file open to
-- know what `2` meant.
--
-- # Why the row survives a deletion
--
-- `shares.user_id` is `ON DELETE SET NULL`, so removing the row would turn
-- every share that account uploaded into one indistinguishable from an
-- anonymous upload — and "nobody ever owned this" is a different fact from
-- "the owner is gone". Once merged they cannot be told apart again.
--
-- Keeping the row also keeps `deleted_by` pointing at somebody, and keeps the
-- username and address out of circulation questions: both are scrubbed on
-- deletion, which frees them for somebody else while leaving nothing personal
-- behind.

ALTER TABLE users ADD COLUMN state VARCHAR(191) NOT NULL DEFAULT 'active';

-- When the account was removed. Null for every account that has not been.
ALTER TABLE users ADD COLUMN deleted_at DATETIME(6);

-- Who removed it, or the account itself when somebody removed their own.
--
-- References `users`, which works because a deletion leaves the row behind. An
-- administrator who is themselves later deleted still has a row for this to
-- point at.
ALTER TABLE users ADD COLUMN deleted_by CHAR(26);
ALTER TABLE users ADD CONSTRAINT fk_users_deleted_by
    FOREIGN KEY (deleted_by) REFERENCES users (id) ON DELETE SET NULL;

-- Read by the listing, which filters deleted accounts out by default.
CREATE INDEX idx_users_state ON users (state);
