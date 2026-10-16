-- Makes every stored time mean the same thing on MySQL: a UTC wall clock.
--
-- # What was wrong with `TIMESTAMP`
--
-- Nothing, for the program. `sqlx` sends `time_zone='+00:00'` on every
-- connection, so a value written and read back is the same instant whatever
-- the server is set to; against a server forced to `+08:00`, the round trip
-- is exact to the microsecond.
--
-- Two things were wrong for everyone else.
--
-- **`TIMESTAMP` stops in 2038.** Not by truncating, which might be noticed
-- later, but by refusing the row:
--
--     UPDATE api_tokens SET expires_at = '2039-01-01 00:00:00';
--     ERROR 1292 (22007): Incorrect datetime value
--
-- `MAX_EXPIRY_DAYS` is 3650, so from January 2028 a caller requesting the
-- longest token this server offers gets a `500`, on MySQL only, the worst
-- kind of difference between backends.
--
-- **The same moment read as two different times.** `TIMESTAMP` converts to
-- the session's zone on the way out, `DATETIME` does not, and the schema had
-- both. An operator in `mysql` saw `16:00` from `api_tokens` and `00:00` from
-- `roles` for the same moment.
--
-- # Why `DATETIME` and not the reverse
--
-- The program already guarantees UTC: every timestamp comes from
-- `OffsetDateTime::now_utc()`, and the `distroless` image carries no zoneinfo
-- at all. A second conversion in the database adds no correctness, only a
-- ceiling and a number that shifts.
--
-- # Why no `SET time_zone` here
--
-- Converting `TIMESTAMP` to `DATETIME` uses the session's zone, so this relies
-- on running as UTC. It does: `sqlx` sets it per connection, which makes this
-- safe even though `execute_all` may use a different pooled connection per
-- statement. A `SET` here would cover only one of them while looking like a
-- guarantee.

ALTER TABLE api_tokens
    MODIFY COLUMN created_at DATETIME(6) NOT NULL,
    MODIFY COLUMN last_used_at DATETIME(6) NULL,
    MODIFY COLUMN expires_at DATETIME(6) NULL,
    MODIFY COLUMN revoked_at DATETIME(6) NULL;

ALTER TABLE file_shares
    MODIFY COLUMN created_at DATETIME(6) NOT NULL;

ALTER TABLE invites
    MODIFY COLUMN created_at DATETIME(6) NOT NULL,
    MODIFY COLUMN expires_at DATETIME(6) NULL;

ALTER TABLE password_reset_tokens
    MODIFY COLUMN created_at DATETIME(6) NOT NULL,
    MODIFY COLUMN expires_at DATETIME(6) NOT NULL,
    MODIFY COLUMN used_at DATETIME(6) NULL;

ALTER TABLE pending_registrations
    MODIFY COLUMN created_at DATETIME(6) NOT NULL,
    MODIFY COLUMN expires_at DATETIME(6) NOT NULL;

ALTER TABLE schema_metadata
    MODIFY COLUMN updated_at DATETIME(6) NOT NULL DEFAULT CURRENT_TIMESTAMP(6);

ALTER TABLE sessions
    MODIFY COLUMN created_at DATETIME(6) NOT NULL,
    MODIFY COLUMN last_seen_at DATETIME(6) NOT NULL,
    MODIFY COLUMN expires_at DATETIME(6) NOT NULL;

ALTER TABLE share_tickets
    MODIFY COLUMN created_at DATETIME(6) NOT NULL,
    MODIFY COLUMN expires_at DATETIME(6) NOT NULL;

ALTER TABLE shares
    MODIFY COLUMN expires_at DATETIME(6) NULL,
    MODIFY COLUMN disabled_at DATETIME(6) NULL,
    MODIFY COLUMN last_accessed_at DATETIME(6) NULL,
    MODIFY COLUMN created_at DATETIME(6) NOT NULL,
    MODIFY COLUMN updated_at DATETIME(6) NOT NULL;

ALTER TABLE users
    MODIFY COLUMN email_verified_at DATETIME(6) NULL,
    MODIFY COLUMN created_at DATETIME(6) NOT NULL,
    MODIFY COLUMN updated_at DATETIME(6) NOT NULL;
