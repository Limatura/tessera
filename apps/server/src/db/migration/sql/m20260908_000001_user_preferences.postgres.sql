-- What one person has chosen about how the site looks to them.
--
-- # Why the identifier is the primary key
--
-- One row per person, always. There is nothing to list, nothing to page
-- through, and no second row that could disagree with the first. Writing is
-- then an upsert on a key the caller already has, which is what makes "replace
-- the whole thing" the only operation this table needs.
--
-- # Why the preferences are one JSON column and not three
--
-- The column holds `{"mode":"dark","theme":"sepia","accent":"rose"}`, and every
-- field of it is optional: absent means the person has no opinion and keeps
-- following whatever the instance's default is, including after the operator
-- changes it. Three columns would say the same thing with three nullable
-- columns, and would need a migration for the fourth choice.
--
-- The names are checked before they arrive, against the lists in
-- `tessera_core::appearance`. Nothing else may be stored, because these end up
-- as attributes on the `<html>` element.
--
-- `TEXT` rather than a JSON type on purpose: `settings.value` is `TEXT` for the
-- same reason, that Postgres `jsonb` would give the entity a different Rust
-- type on one dialect out of three.
--
-- # Why the table is not called `user_appearance`
--
-- Language, time zone and whether a clock reads 13:00 or 1pm are all preferences
-- that are not appearance, and all of them will want somewhere to live. Naming
-- the table for the first column would mean a second table, or a rename, the
-- first time one of them arrives.
CREATE TABLE IF NOT EXISTS user_preferences (
    user_id    TEXT NOT NULL PRIMARY KEY
               REFERENCES users (id) ON DELETE CASCADE,
    -- The appearance choices as JSON. `{}` and an absent row mean the same
    -- thing, and both are what somebody who has never opened the setting has.
    appearance TEXT NOT NULL DEFAULT '{}',
    -- Every other table in this schema stores a time as `TIMESTAMPTZ`.
    updated_at TIMESTAMPTZ NOT NULL
);
