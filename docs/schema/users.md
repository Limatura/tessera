# users

## Description

Accounts. An empty `role_id` means signed in and allowed nothing, which is a real state rather than a fault. A deleted account leaves a scrubbed row behind rather than disappearing, because `shares.user_id` points at it and "nobody uploaded this" is a different fact from "whoever uploaded it is gone".

<details>
<summary><strong>Table Definition</strong></summary>

```sql
CREATE TABLE users (
    id            TEXT    NOT NULL PRIMARY KEY,
    username      TEXT    NOT NULL,
    email         TEXT    NOT NULL,
    must_change_password INTEGER NOT NULL DEFAULT 0,
    -- TEXT, not INTEGER: sqlx encodes an `OffsetDateTime` as RFC 3339 text for
    -- SQLite, and a column declared INTEGER would accept it anyway (SQLite types
    -- are advisory) while misrepresenting what is stored. Always UTC and always
    -- the same width, so lexicographic order matches chronological order.
    email_verified_at    TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
, state TEXT NOT NULL DEFAULT 'active', deleted_at TEXT, deleted_by TEXT REFERENCES users (id) ON DELETE SET NULL, role_id TEXT REFERENCES roles (id) ON DELETE SET NULL, quota_bytes INTEGER, max_file_bytes INTEGER)
```

</details>

## Columns

| Name | Type | Default | Nullable | Children | Parents | Comment |
| ---- | ---- | ------- | -------- | -------- | ------- | ------- |
| id | TEXT |  | false | [users](users.md) [sessions](sessions.md) [api_tokens](api_tokens.md) [shares](shares.md) [password_reset_tokens](password_reset_tokens.md) [uploads](uploads.md) [identities](identities.md) [credentials](credentials.md) [auth_flows](auth_flows.md) [user_preferences](user_preferences.md) [email_verifications](email_verifications.md) |  |  |
| username | TEXT |  | false |  |  |  |
| email | TEXT |  | false |  |  |  |
| must_change_password | INTEGER | 0 | false |  |  |  |
| email_verified_at | TEXT |  | true |  |  |  |
| created_at | TEXT |  | false |  |  |  |
| updated_at | TEXT |  | false |  |  |  |
| state | TEXT | 'active' | false |  |  |  |
| deleted_at | TEXT |  | true |  |  |  |
| deleted_by | TEXT |  | true |  | [users](users.md) |  |
| role_id | TEXT |  | true |  | [roles](roles.md) |  |
| quota_bytes | INTEGER |  | true |  |  |  |
| max_file_bytes | INTEGER |  | true |  |  |  |

## Constraints

| Name | Type | Definition |
| ---- | ---- | ---------- |
| id | PRIMARY KEY | PRIMARY KEY (id) |
| - (Foreign key ID: 0) | FOREIGN KEY | FOREIGN KEY (role_id) REFERENCES roles (id) ON UPDATE NO ACTION ON DELETE SET NULL MATCH NONE |
| - (Foreign key ID: 1) | FOREIGN KEY | FOREIGN KEY (deleted_by) REFERENCES users (id) ON UPDATE NO ACTION ON DELETE SET NULL MATCH NONE |
| sqlite_autoindex_users_1 | PRIMARY KEY | PRIMARY KEY (id) |

## Indexes

| Name | Definition |
| ---- | ---------- |
| idx_users_role | CREATE INDEX idx_users_role ON users (role_id) |
| idx_users_state | CREATE INDEX idx_users_state ON users (state) |
| idx_users_email | CREATE UNIQUE INDEX idx_users_email ON users (email) |
| idx_users_username | CREATE UNIQUE INDEX idx_users_username ON users (username) |
| sqlite_autoindex_users_1 | PRIMARY KEY (id) |

## Relations

```mermaid
erDiagram

"users" }o--o| "users" : ""
"sessions" }o--|| "users" : ""
"api_tokens" }o--|| "users" : ""
"shares" }o--o| "users" : ""
"password_reset_tokens" }o--|| "users" : ""
"uploads" }o--o| "users" : ""
"identities" }o--|| "users" : ""
"credentials" |o--|| "users" : ""
"auth_flows" }o--o| "users" : ""
"user_preferences" |o--|| "users" : ""
"email_verifications" }o--|| "users" : ""
"users" }o--o| "roles" : ""

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
"sessions" {
  TEXT id PK
  TEXT user_id FK
  TEXT secret_hash
  TEXT created_at
  TEXT last_seen_at
  TEXT expires_at
  TEXT created_ip
  TEXT created_user_agent
  TEXT ended_reason
  TEXT origin
  TEXT provider_sid
}
"api_tokens" {
  TEXT id PK
  TEXT user_id FK
  TEXT name
  TEXT secret_hash
  TEXT display_prefix
  TEXT created_at
  TEXT last_used_at
  TEXT expires_at
  TEXT revoked_at
}
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
}
"password_reset_tokens" {
  TEXT id PK
  TEXT user_id FK
  TEXT secret_hash
  TEXT created_at
  TEXT expires_at
  TEXT used_at
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
"identities" {
  TEXT provider PK
  TEXT subject PK
  TEXT user_id FK
  TEXT created_at
}
"credentials" {
  TEXT user_id PK
  TEXT password_hash
  TEXT created_at
  TEXT updated_at
}
"auth_flows" {
  TEXT state PK
  TEXT provider
  TEXT nonce
  TEXT verifier
  TEXT purpose
  TEXT user_id FK
  TEXT invite_code
  TEXT created_at
  TEXT expires_at
  TEXT return_to
}
"user_preferences" {
  TEXT user_id PK
  TEXT appearance
  TEXT updated_at
}
"email_verifications" {
  TEXT id PK
  TEXT user_id FK
  TEXT email
  TEXT secret_hash
  TEXT created_at
  TEXT expires_at
  TEXT used_at
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
