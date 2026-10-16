-- One user's choices about how the site looks to them.
--
-- # Why the user id is the primary key
--
-- One row per user, always: nothing to list, nothing to page, and no second
-- row to disagree with the first. Writing is an upsert on a key the caller
-- already has, which makes "replace the whole thing" the only operation this
-- table needs.
--
-- # Why one JSON column and not three
--
-- The column holds `{"mode":"dark","theme":"sepia","accent":"rose"}`, and
-- every field is optional: absent means no opinion, following the instance
-- default even after the operator changes it. Three nullable columns would say
-- the same and need a migration for a fourth choice.
--
-- The names are validated before they arrive, against the lists in
-- `tessera_core::appearance`. Nothing else may be stored, because these end up
-- as attributes on the `<html>` element.
--
-- `TEXT` instead of a JSON type, by design: `settings.value` is `TEXT` for the
-- same reason, since Postgres `jsonb` would give the entity a different Rust
-- type on one dialect of three.
--
-- # Why the table is not called `user_appearance`
--
-- Language, time zone and 12- or 24-hour clocks are preferences that are not
-- appearance, and each will need a home. Naming the table after its first
-- column would mean a second table, or a rename, when one of them arrives.
CREATE TABLE IF NOT EXISTS user_preferences (
    user_id    TEXT NOT NULL PRIMARY KEY
               REFERENCES users (id) ON DELETE CASCADE,
    -- The appearance choices as JSON. `{}` and a missing row mean the same, and
    -- both describe a user who never opened the setting.
    appearance TEXT NOT NULL DEFAULT '{}',
    -- Every other table in this schema stores a time as `TIMESTAMPTZ`.
    updated_at TIMESTAMPTZ NOT NULL
);
