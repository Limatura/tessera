# shares

## Description

One share, whatever it holds. The code, the password, the counters and the expiry live here because those rules do not depend on the content. The content does not: it is in `file_shares` and `text_shares`, because a share can hold several files and a column cannot hold several. `role_id` is the role at the moment of upload, `guest` for anonymous, so changing somebody's role later does not retroactively change who an old share counted against. `user_id` being empty means either anonymous or an uploader whose account was deleted, and nothing tells the two apart. `bytes_served` counts bytes that left this process, not bytes that arrived: a proxy in front inflates it by whatever it holds, and a request a CDN answered is never seen at all. `bytes_redirected` is what downloads sent to the store asked for: requested, not delivered, and kept apart from `bytes_served`, which counts only what left this process.

<details>
<summary><strong>Table Definition</strong></summary>

```sql
CREATE TABLE shares (
    id             TEXT    NOT NULL PRIMARY KEY,
    -- The public retrieval code, as it appears in `/s/{code}`. Separate from the
    -- primary key so it can be rotated without rewriting foreign keys, and so
    -- inserts keep the locality a ULID primary key gives us.
    code           TEXT    NOT NULL UNIQUE,
    -- NULL for an anonymous upload. `ON DELETE SET NULL`, not CASCADE: what
    -- happens to a departing account's shares is not decided yet, and orphaning a
    -- row is recoverable while deleting it is not. Deleting would also strand the
    -- objects, since no database action reaches the blob store.
    user_id        TEXT    REFERENCES users (id) ON DELETE SET NULL,
    kind           TEXT    NOT NULL,

    -- Everything the access gate reads.
    password_hash  TEXT,
    -- NULL means unlimited. A count plus a limit instead of a countdown: a
    -- countdown discards how many times something was actually fetched, and that
    -- number cannot be reconstructed later.
    use_limit      INTEGER,
    use_count      INTEGER NOT NULL DEFAULT 0,
    expires_at     TEXT,
    disabled_at    TEXT,

    -- Presentation.
    title          TEXT,
    og_config      TEXT,
    -- Lets an anonymous uploader delete what they just posted. Without it a
    -- mistaken upload is permanent.
    management_token_hash TEXT,
    view_count     INTEGER NOT NULL DEFAULT 0,
    last_accessed_at TEXT,

    created_at     TEXT    NOT NULL,
    updated_at     TEXT    NOT NULL
, copy_count INTEGER NOT NULL DEFAULT 0, bytes_served INTEGER NOT NULL DEFAULT 0, role_id TEXT REFERENCES roles (id) ON DELETE SET NULL, bytes_redirected INTEGER NOT NULL DEFAULT 0, encryption TEXT, key_salt TEXT, sealed_details TEXT, end_announced_at TEXT)
```

</details>

## Columns

| Name | Type | Default | Nullable | Children | Parents | Comment |
| ---- | ---- | ------- | -------- | -------- | ------- | ------- |
| id | TEXT |  | false | [share_tickets](share_tickets.md) [file_shares](file_shares.md) [text_shares](text_shares.md) [uploads](uploads.md) [access_events](access_events.md) |  |  |
| code | TEXT |  | false |  |  |  |
| user_id | TEXT |  | true |  | [users](users.md) |  |
| kind | TEXT |  | false |  |  |  |
| password_hash | TEXT |  | true |  |  |  |
| use_limit | INTEGER |  | true |  |  |  |
| use_count | INTEGER | 0 | false |  |  |  |
| expires_at | TEXT |  | true |  |  |  |
| disabled_at | TEXT |  | true |  |  |  |
| title | TEXT |  | true |  |  |  |
| og_config | TEXT |  | true |  |  |  |
| management_token_hash | TEXT |  | true |  |  |  |
| view_count | INTEGER | 0 | false |  |  |  |
| last_accessed_at | TEXT |  | true |  |  |  |
| created_at | TEXT |  | false |  |  |  |
| updated_at | TEXT |  | false |  |  |  |
| copy_count | INTEGER | 0 | false |  |  |  |
| bytes_served | INTEGER | 0 | false |  |  |  |
| role_id | TEXT |  | true |  | [roles](roles.md) |  |
| bytes_redirected | INTEGER | 0 | false |  |  |  |
| encryption | TEXT |  | true |  |  |  |
| key_salt | TEXT |  | true |  |  |  |
| sealed_details | TEXT |  | true |  |  |  |
| end_announced_at | TEXT |  | true |  |  |  |

## Constraints

| Name | Type | Definition |
| ---- | ---- | ---------- |
| id | PRIMARY KEY | PRIMARY KEY (id) |
| - (Foreign key ID: 0) | FOREIGN KEY | FOREIGN KEY (role_id) REFERENCES roles (id) ON UPDATE NO ACTION ON DELETE SET NULL MATCH NONE |
| - (Foreign key ID: 1) | FOREIGN KEY | FOREIGN KEY (user_id) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE SET NULL MATCH NONE |
| sqlite_autoindex_shares_2 | UNIQUE | UNIQUE (code) |
| sqlite_autoindex_shares_1 | PRIMARY KEY | PRIMARY KEY (id) |

## Indexes

| Name | Definition |
| ---- | ---------- |
| idx_shares_end_announced | CREATE INDEX idx_shares_end_announced ON shares (end_announced_at, expires_at) |
| idx_shares_role | CREATE INDEX idx_shares_role ON shares (role_id) |
| idx_shares_created | CREATE INDEX idx_shares_created ON shares (created_at) |
| idx_shares_user_created | CREATE INDEX idx_shares_user_created ON shares (user_id, created_at) |
| idx_shares_expires_at | CREATE INDEX idx_shares_expires_at ON shares (expires_at) |
| sqlite_autoindex_shares_2 | UNIQUE (code) |
| sqlite_autoindex_shares_1 | PRIMARY KEY (id) |

## Relations

```mermaid
erDiagram

"share_tickets" }o--|| "shares" : ""
"file_shares" }o--|| "shares" : ""
"text_shares" |o--|| "shares" : ""
"uploads" }o--o| "shares" : ""
"access_events" }o--|| "shares" : ""
"shares" }o--o| "users" : ""
"shares" }o--o| "roles" : ""

"shares" {
  TEXT id PK
  TEXT code
  TEXT user_id FK
  TEXT kind
  TEXT password_hash
  INTEGER use_limit
  INTEGER use_count
  TEXT expires_at
  TEXT disabled_at
  TEXT title
  TEXT og_config
  TEXT management_token_hash
  INTEGER view_count
  TEXT last_accessed_at
  TEXT created_at
  TEXT updated_at
  INTEGER copy_count
  INTEGER bytes_served
  TEXT role_id FK
  INTEGER bytes_redirected
  TEXT encryption
  TEXT key_salt
  TEXT sealed_details
  TEXT end_announced_at
}
"share_tickets" {
  TEXT id PK
  TEXT share_id FK
  TEXT secret_hash
  TEXT created_at
  TEXT expires_at
}
"file_shares" {
  TEXT id PK
  TEXT share_id FK
  INTEGER position
  TEXT filename
  TEXT content_type
  INTEGER size_bytes
  TEXT storage
  TEXT object_key
  TEXT created_at
  TEXT sealed_name
  TEXT scan_state
  TEXT scan_detail
  TEXT detected_type
}
"text_shares" {
  TEXT share_id PK
  TEXT language
  BLOB content
  TEXT scan_state
  TEXT scan_detail
}
"uploads" {
  TEXT id PK
  TEXT share_id FK
  TEXT user_id FK
  TEXT token_hash
  TEXT filename
  TEXT content_type
  INTEGER declared_bytes
  INTEGER offset_bytes
  TEXT created_at
  TEXT updated_at
  TEXT expires_at
  TEXT role_id FK
  TEXT encryption
  TEXT key_salt
  TEXT sealed_name
}
"access_events" {
  TEXT id PK
  TEXT share_id FK
  TEXT occurred_at
  TEXT ip_address
  TEXT user_agent
}
"users" {
  TEXT id PK
  TEXT username
  TEXT email
  INTEGER must_change_password
  TEXT email_verified_at
  TEXT created_at
  TEXT updated_at
  TEXT state
  TEXT deleted_at
  TEXT deleted_by FK
  TEXT role_id FK
  INTEGER quota_bytes
  INTEGER max_file_bytes
}
"roles" {
  TEXT id PK
  TEXT name
  TEXT description
  INTEGER is_system
  TEXT created_at
  TEXT updated_at
  INTEGER quota_bytes
  INTEGER max_file_bytes
}
```

---

> Generated by [tbls](https://github.com/k1LoW/tbls)
