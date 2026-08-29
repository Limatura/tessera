# Migrations

Three databases are supported: SQLite, PostgreSQL, MySQL. Each migration is hand-written SQL, one file per dialect.

Code: `db/migration/mod.rs`. Files: `db/migration/sql/`, rollbacks in `db/migration/sql/down/`.

## Why hand-written SQL

SeaORM's schema builder renders correct DDL for every dialect from one definition, which is the obvious thing to reach for.

The table definition is the part people read. A contributor checking what a migration does, or an operator confirming what their database looks like, should see

```sql
CREATE TABLE shares (id TEXT PRIMARY KEY, ...);
```

not a chain of `ColumnDef::new(...).text().not_null()`. SQL files can be read at a glance, diffed meaningfully in review, and pasted into a shell to check a hypothesis.

The cost is three files per migration. That is affordable because migrations are rare. Queries, which change often, are still written once through SeaORM.

The risk is a migration updated for one dialect and not the others. CI applies all three and compares the resulting schemas.

## Naming

`m{YYYYMMDD}_{NNNNNN}_{name}.{dialect}.sql`, with a matching file in `down/`.

## Dialect traps

Each of these cost real time. They are here so the next person does not pay again.

### MySQL

**Column-level `REFERENCES` is silently ignored.** InnoDB parses it and does nothing. Foreign keys must be declared as table-level `FOREIGN KEY (...) REFERENCES ...` constraints.

**`TEXT` needs a prefix length in a key.** `KEY (some_text_column)` is an error; `KEY (some_text_column(191))` works. This is why several columns are `VARCHAR(n)` rather than `TEXT`.

**`DROP INDEX IF EXISTS` is not valid syntax.** Neither is `CREATE INDEX IF NOT EXISTS`. Rollbacks have to know what exists.

**Changing a primary key must be one statement.** `ALTER TABLE t DROP PRIMARY KEY, ADD PRIMARY KEY (...)`. As two statements it fails, because a foreign key needs an index on the referenced column throughout, and between the two statements there is none.

**`UPDATE` cannot read the table it updates via a subquery.** Rewrite as a JOIN.

### SQLite

**A primary key cannot be altered.** The table must be rebuilt: create the new one, copy, drop the old, rename.

**A name cannot be freed and reused in the same transaction.** Dropping `upload_parts` and then renaming a new table to `upload_parts` in one transaction fails. Rename the old table out of the way first, then rename the new one in.

## Rollbacks

Most migrations implement `down` inline in their `.rs` file, using one portable SQL string through `execute_portable`.

Two of them needed per-dialect rollback SQL, and those files live in `sql/down/` rather than beside the migration. That is not tidiness:

`scripts/schema-docs.py` builds the schema it documents by replaying every `sql/*.sqlite.sql` in name order. A rollback left in that directory sorts **after** the migration it undoes, because `.` sorts before `_`. The generated schema would come out rolled back, and the `checks` job would pass because the files match what the script produced. Nothing else would catch it.

`a_full_rollback_runs_on_this_dialect` in `tests/backends.rs` runs the whole down path, and the CI matrix runs it against PostgreSQL and MySQL as well as SQLite.

Four rollbacks were broken on MySQL and only found by actually running them.

## Testing

CI runs migrations against live PostgreSQL and MySQL containers, not just SQLite. Several bugs shipped past SQLite-only testing and were caught by that job.
