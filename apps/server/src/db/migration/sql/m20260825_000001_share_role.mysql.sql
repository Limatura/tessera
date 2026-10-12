-- Which role uploaded a share.
--
-- # Why a column and not a join
--
-- The quota a share counts against is decided by role, and the upload
-- path would otherwise look up the account on every request to find it. One
-- column answers that, and summing a role's usage becomes one indexed query
-- instead of a join.
--
-- # A snapshot, by design
--
-- This is the uploader's role *at upload time*. Moving the account to another
-- role later does not change which quota its old shares counted against;
-- otherwise one change to one account would silently redistribute storage
-- already accounted for.
--
-- # Why anonymous uploads get `guest` instead of NULL
--
-- `user_id IS NULL` already means two things: no one was signed in, or the
-- uploading account has since been deleted. Inferring "anonymous" from it is
-- wrong in the second case. The role distinguishes them, and `guest` is the
-- seeded role for this.
--
-- # ON DELETE SET NULL
--
-- A role can be removed while shares uploaded under it still exist. Losing
-- which role is recoverable; refusing to delete the role, or deleting shares
-- along with it, is not. A share with no role falls back to the instance-wide
-- limit, as do all shares that predate this column.
--
-- # Why this dialect looks different
--
-- `VARCHAR(191)` because the column is indexed and MySQL cannot index TEXT
-- without a prefix length. The foreign key is table-level because `InnoDB`
-- silently ignores a `REFERENCES` on a column.
ALTER TABLE shares ADD COLUMN role_id VARCHAR(191);
ALTER TABLE shares ADD CONSTRAINT fk_shares_role FOREIGN KEY (role_id) REFERENCES roles (id) ON DELETE SET NULL;

CREATE INDEX idx_shares_role ON shares (role_id);
