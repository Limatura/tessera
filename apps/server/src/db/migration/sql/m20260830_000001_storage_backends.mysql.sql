-- Storage backends an operator can add without restarting the process.
--
-- # Why these are rows and not settings
--
-- `storage.url` is in `SECRETS`, so it never reaches the settings table and
-- can only be changed by restarting with a different environment. That was
-- fine while there was one; it is not fine as the way to add the second, third
-- and fourth, which is the whole point of having them.
--
-- # Why the credentials are sealed
--
-- A URL like `s3://key:secret@host` has to be *recoverable* — the process
-- needs it to reach the bucket — which makes a database dump a credential leak
-- unless what opens it lives somewhere the dump does not. The key is
-- `security.secret_key`, from the environment. Without one, adding a backend is
-- refused rather than written in plaintext.
--
-- # Why the name is the key
--
-- `file_segments.backend` names one of these, and that reference has to stay
-- readable when the row is gone — a share pointing at a backend nobody
-- configured any more is something an operator must be told about, not a
-- foreign-key error. So the name is stable and there is no key across.
CREATE TABLE IF NOT EXISTS storage_backends (
    name        VARCHAR(191) NOT NULL PRIMARY KEY,
    -- Sealed. See `storage::secret`.
    url         TEXT NOT NULL,
    -- Sealed JSON object of backend-specific options.
    options     TEXT NOT NULL,
    -- Whether new uploads go here. Exactly one row should have it.
    is_default  BOOLEAN    NOT NULL DEFAULT FALSE,
    created_at  DATETIME(6)   NOT NULL,
    updated_at  DATETIME(6)   NOT NULL
);
