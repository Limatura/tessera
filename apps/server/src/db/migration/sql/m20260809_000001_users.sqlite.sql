-- Accounts.
--
-- `id` is a ULID in its 26-character text form, not an integer: sequential ids
-- would let anyone count and enumerate accounts, and this service may face the
-- internet.
--
-- `username` and `email` are stored already normalised by the domain layer
-- (username lowercased; email domain lowercased, local part untouched), so a
-- plain UNIQUE index is enough and the database never needs the rules.
CREATE TABLE IF NOT EXISTS users (
    id            TEXT    NOT NULL PRIMARY KEY,
    username      TEXT    NOT NULL,
    email         TEXT    NOT NULL,
    password_hash TEXT    NOT NULL,
    is_admin      INTEGER NOT NULL DEFAULT 0,
    -- Set when the account must change its password before doing anything else,
    -- which is how the generated first-run administrator is handed over safely.
    must_change_password INTEGER NOT NULL DEFAULT 0,
    -- TEXT, not INTEGER: sqlx encodes an `OffsetDateTime` as RFC 3339 text for
    -- SQLite, and a column declared INTEGER would accept it anyway (SQLite types
    -- are advisory) while misrepresenting what is stored. Always UTC and always
    -- the same width, so lexicographic order matches chronological order.
    email_verified_at    TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username ON users (username);

CREATE UNIQUE INDEX IF NOT EXISTS idx_users_email ON users (email);
