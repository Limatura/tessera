-- Storage backends an operator can add without restarting the process.
--
-- # Why these are rows and not settings
--
-- `storage.url` is in `SECRETS`, so it never reaches the settings table and
-- can only change by restarting with a different environment. That was fine
-- for one backend, but not as the way to add a second, third and fourth,
-- the point of having several.
--
-- # Why the credentials are sealed
--
-- A URL like `s3://key:secret@host` must be *recoverable* (the process needs
-- it to reach the bucket), which makes a database dump a credential leak
-- unless the key lives somewhere the dump does not. The key is
-- `security.secret_key`, from the environment. Without one, adding a backend
-- is refused instead of stored in plaintext.
--
-- # Why the name is the key
--
-- `file_segments.backend` names one of these, and that reference must stay
-- readable after the row is gone: a share pointing at a backend no longer
-- configured is something to report to the operator, not a foreign-key
-- error. So the name is stable and there is no foreign key.
CREATE TABLE IF NOT EXISTS storage_backends (
    name        TEXT NOT NULL PRIMARY KEY,
    -- Sealed. See `storage::secret`.
    url         TEXT NOT NULL,
    -- Sealed JSON object of backend-specific options.
    options     TEXT NOT NULL,
    -- Whether new uploads go here. Exactly one row should have it.
    is_default  INTEGER    NOT NULL DEFAULT FALSE,
    created_at  TEXT   NOT NULL,
    updated_at  TEXT   NOT NULL
);
