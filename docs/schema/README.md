# tessera

## Tables

| Name | Columns | Comment | Type |
| ---- | ------- | ------- | ---- |
| [schema_metadata](schema_metadata.md) | 3 | 這個資料庫是哪一版寫的。降版啟動時用來擋下來。 | table |
| [users](users.md) | 11 | 帳號。刪掉的不會消失,只會變成一列洗乾淨的殘骸——因為 shares.user_id 指著它,而「沒有人上傳過」和「上傳的人不在了」是兩件不同的事。 | table |
| [sessions](sessions.md) | 9 | 一次登入。過期了也留著,因為這張表同時是登入紀錄——使用者要看得出有沒有別人進來過。真正的清除靠 `auth.login_history_days`。 | table |
| [api_tokens](api_tokens.md) | 9 | 給程式用的憑證。只存摘要,原始值發出去之後就不再存在。 | table |
| [shares](shares.md) | 18 | 一次分享,不管內容是什麼。取用碼、密碼、次數、到期都在這裡,因為那些規則跟內容是什麼無關。內容完全不在這裡——它在 file_shares 和 text_shares,因為一份分享可以有好幾個檔案,而欄位裝不下「好幾個」。bytes_served 是「離開這個 process 的位元組」,不是「到達使用者的」——前面掛 proxy 的話它握著的那一小段會被多算,而 CDN 快取掉的請求根本不會被看到。 | table |
| [share_tickets](share_tickets.md) | 5 | 輸對密碼之後拿到的通行證。存在的理由是「看得到」和「拿得到」要分開——預覽頁不該花掉一次取用次數。 | table |
| [password_reset_tokens](password_reset_tokens.md) | 6 | 重設密碼的一次性連結。 | table |
| [pending_registrations](pending_registrations.md) | 8 | 還沒驗證的註冊。帳號在信件被點開之前不存在,所以資料先放這裡——這樣「這個 email 註冊了沒」問不出來。 | table |
| [invites](invites.md) | 7 | 邀請碼。同樣只存摘要。 | table |
| [file_shares](file_shares.md) | 9 | 檔案分享的每一個檔案。現在一次分享一列,#42 之後會是多列,`position` 就是為此存在的。 | table |
| [text_shares](text_shares.md) | 3 | 文字分享的內容本身。`content` 不可為空——「文字的內容在別的地方」正是這張表要消滅的狀態。 | table |
| [roles](roles.md) | 6 | 一組權限的名字。誰能做什麼只由這裡決定,沒有別的地方。 | table |
| [role_permissions](role_permissions.md) | 2 | 角色包含哪些權限。權限本身是程式定義的,不是資料——沒有人檢查的權限給了也是空的。 | table |
| [user_roles](user_roles.md) | 3 | 誰在哪些角色裡。沒有任何一列 = 登入了但什麼都不能做,那是真的狀態不是錯誤。 | table |
| [api_token_permissions](api_token_permissions.md) | 2 | 一張 token 能做什麼。實際生效的是這裡跟主人權限的交集,所以主人被降權,他手上的 token 同時就窄了。 | table |
| [uploads](uploads.md) | 13 | 還在傳的檔案。一份還沒傳完的東西不是 file_shares 的一列——那張表的每一列都代表「拿得到的內容」,而這裡的東西還拿不到。object_key 開檔時就決定,之後不再重算,因為續傳要能接回同一個位置。declared_bytes 是客戶端宣告的大小,配額算的是這個而不是已經到的,不然兩個人可以同時宣告用光同一份空間。 | table |
| [upload_parts](upload_parts.md) | 3 | 分片上傳裡 store 已經收下的每一片。存著是因為 complete_multipart 要把每一片原封不動交回去,而沒有一種列法是三家雲端都同意的。本地檔案系統不會有這裡的列——它用 append。 | table |

## Relations

```mermaid
erDiagram

"users" }o--o| "users" : ""
"sessions" }o--|| "users" : ""
"api_tokens" }o--|| "users" : ""
"shares" }o--o| "users" : ""
"share_tickets" }o--|| "shares" : ""
"password_reset_tokens" }o--|| "users" : ""
"file_shares" }o--|| "shares" : ""
"text_shares" |o--|| "shares" : ""
"role_permissions" |o--|| "roles" : ""
"user_roles" |o--|| "roles" : ""
"user_roles" |o--|| "users" : ""
"api_token_permissions" |o--|| "api_tokens" : ""
"uploads" }o--o| "users" : ""
"uploads" }o--o| "shares" : ""
"upload_parts" |o--|| "uploads" : ""

"schema_metadata" {
  TEXT key PK
  TEXT value
  INTEGER updated_at
}
"users" {
  TEXT id PK
  TEXT username
  TEXT email
  TEXT password_hash
  INTEGER must_change_password
  TEXT email_verified_at
  TEXT created_at
  TEXT updated_at
  TEXT state
  TEXT deleted_at
  TEXT deleted_by FK
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
}
"share_tickets" {
  TEXT id PK
  TEXT share_id FK
  TEXT secret_hash
  TEXT created_at
  TEXT expires_at
}
"password_reset_tokens" {
  TEXT id PK
  TEXT user_id FK
  TEXT secret_hash
  TEXT created_at
  TEXT expires_at
  TEXT used_at
}
"pending_registrations" {
  TEXT id PK
  TEXT username
  TEXT email
  TEXT password_hash
  TEXT secret_hash
  TEXT created_at
  TEXT expires_at
  TEXT invite_id
}
"invites" {
  TEXT id PK
  TEXT code_hash
  TEXT note
  INTEGER max_uses
  INTEGER used
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
}
"text_shares" {
  TEXT share_id PK
  TEXT language
  BLOB content
}
"roles" {
  TEXT id PK
  TEXT name
  TEXT description
  INTEGER is_system
  TEXT created_at
  TEXT updated_at
}
"role_permissions" {
  TEXT role_id PK
  TEXT permission PK
}
"user_roles" {
  TEXT user_id PK
  TEXT role_id PK
  TEXT granted_at
}
"api_token_permissions" {
  TEXT token_id PK
  TEXT permission PK
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
  TEXT object_key
  TEXT multipart_id
  TEXT created_at
  TEXT updated_at
  TEXT expires_at
}
"upload_parts" {
  TEXT upload_id PK
  INTEGER part_number PK
  TEXT content_id
}
```

---

> Generated by [tbls](https://github.com/k1LoW/tbls)
