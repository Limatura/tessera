-- What one person has chosen about how the site looks to them. The SQLite file
-- carries the reasoning; this one differs only in types.
CREATE TABLE IF NOT EXISTS user_preferences (
    -- `TEXT` cannot be a primary key here without a prefix length, and
    -- `DATETIME(6)` is what every other table in this schema stores a time as.
    user_id    VARCHAR(191) NOT NULL PRIMARY KEY,
    -- The appearance choices as JSON. `{}` and an absent row mean the same
    -- thing. `TEXT` cannot carry a `DEFAULT` in MySQL, so the default is the
    -- writer's job; it always writes the column.
    appearance TEXT         NOT NULL,
    updated_at DATETIME(6)  NOT NULL,
    -- A column-level `REFERENCES` is parsed and ignored by InnoDB.
    CONSTRAINT fk_user_preferences_user FOREIGN KEY (user_id)
        REFERENCES users (id) ON DELETE CASCADE
);
