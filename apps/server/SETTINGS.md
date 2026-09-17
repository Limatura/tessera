# 設定 API 可設定的參數

來源：[config.rs](src/config.rs)、[settings.rs](src/settings.rs)、[api/settings.rs](src/api/settings.rs)

## API

- `GET /admin/settings` — 列出所有設定（含不可改的，標 `editable: false` + `refusal` 原因）
- `PUT /admin/settings` — 改設定，`changes: {key: value}`，`value: null` 清除回退到環境變數/預設值
- `POST /admin/settings/preview` — 預覽改動會刪掉什麼（如縮短保留天數），不會真的套用

改設定需要 `SettingsWrite` 權限（`preview`/`GET` 用 `SettingsRead`），且要 `features.admin` 有開。

## 不可透過面板改的設定（`FIXED_AT_BOOT` / `SECRETS`）

| Key | 原因 |
|---|---|
| `server.host` / `server.port` | listener 已綁定 |
| `database.*` | 讀 settings table 本身要先有連線池 |
| `telemetry.*` | log subscriber 早於一切被安裝 |
| `server.cors_allowed_origins` | 建路由時就烤進 middleware |
| `server.max_body_bytes` | 同上，且只有一半邏輯會跟著改 |
| `server.assets_dir` | SPA 開機時讀一次就內嵌了 |
| `features.docs` | 建路由時決定要不要掛 |
| `auth.admin_password` | 只在首次啟動用一次，存起來沒意義 |
| `mail.username` / `mail.password` | 密碼/API key |
| `database.url` / `storage.url` | URL 裡帶帳密 |

以上其餘的欄位（下面全部）都可透過 `PUT /admin/settings` 即時生效，不用重啟。

## 可設定參數

### `server.*`（除上表外，可讀不可改的欄位不列）
- `trusted_proxies: string[]` — 信任的 proxy CIDR/IP 清單，預設 `[]`
- `trusted_proxy: bool`（deprecated，用 `trusted_proxies`）— 預設 `false`
- `request_timeout_secs: u64` — 一般請求逾時秒數，預設 `30`
- `upload_stall_secs: u64` — 上傳無資料多久算斷線，預設 `30`
- `part_size_bytes: usize` — multipart 上傳分片大小，預設 `8 MiB`

### `database.*`（全部 fixed at boot，僅能用環境變數）
- `url`, `max_connections`（預設 `8`）, `acquire_timeout_secs`（預設 `10`）, `run_migrations`（預設 `true`）

### `storage.*`
- `url: string` — blob 位置（本地路徑或 `s3://` 等），預設 `data/blobs`
- `options: map<string,string>` — backend 專屬選項
- `max_total_bytes: u64` — 所有 share 內容總量上限，`0` = 無限制，預設 `0`

### `features.*`
- `accounts: bool` — 帳號/登入功能，預設 `true`
- `admin: bool` — 管理端點，預設 `true`
- `anonymous_upload: bool` — 允許匿名上傳，預設 `true`
- `docs: bool`（fixed at boot）— `/docs` 頁面，預設 `false`

### `auth.*`
- `registration: "open" | "invite" | "closed"` — 誰能註冊，預設 `open`
- `session_ttl_hours: u64` — session 存活時數，預設 `720`（30 天）
- `login_history_days: u64` — 登入紀錄保留天數，預設 `90`
- `default_role: string` — 新註冊帳號的角色，預設 `member`
- `admin_username / admin_email: string` — 首次啟動建立的管理員資訊
- `cookie_secure: SecurePolicy` — session cookie 何時標 `Secure`
- `password.*`：
  - `min_length: usize` — 最短字數，預設 `8`，下限 `8`
  - `max_length: usize` — 最長字數，預設 `256`，上限 `1024`
  - `require_mixed_case: bool` — 要有大寫跟小寫字母，預設 `false`
  - `require_digit: bool` — 要有數字，預設 `false`
  - `require_symbol: bool` — 要有符號（空白不算），預設 `false`
- `hashing.*`（Argon2id）：
  - `memory_kib: u32` — 預設 `19456`（19 MiB）
  - `iterations: u32` — 預設 `2`
  - `lanes: u32` — 預設 `1`
- `throttle.*`：
  - `max_tracked_keys: usize` — 預設值見 `throttle::MAX_TRACKED_KEYS`，上限 `100_000`
  - `login / source / lookup / reset / registration / share`：各自 `{max_attempts, window_secs}`

### `share.*`
- `code_length: usize` — 取件碼字數，預設 `8`
- `inline_threshold_bytes: usize` — 多小以內存資料庫而非物件儲存，預設 `64 KiB`
- `max_text_bytes: usize` — 純文字分享上限，預設 `1 MiB`
- `ticket_ttl_hours: u64` — 下載授權最長存活，預設 `24`
- `ticket_idle_hours: u64` — 下載授權閒置多久失效，預設 `1`
- `default_expiry_hours: u64` — 未指定到期時間的預設值，`0` = 用 `max_expiry_hours`，預設 `168`（7 天）
- `max_expiry_hours: u64` — 使用者可要求的最長到期時間，`0` = 無上限，預設 `720`（30 天）
- `retain_after_death_hours: u64` — 過期/用完的分享保留多久才真的刪，預設 `168`（7 天）
- `upload_session_ttl_hours: u64` — 未完成上傳的存活時間，預設 `24`
- `access_log.*`：
  - `enabled: bool` — 是否記錄每次取件，預設 `false`
  - `retention_days: u64` — 記錄保留天數，開啟時不可為 `0`，預設 `30`

### `mail.*`（`username`/`password` 為 secret，不可透過面板改）
- `host: string` — 空字串代表關閉寄信功能
- `port: u16` — 預設 `587`
- `from: string` — 寄件地址
- `public_url: string` — 用來組信件內連結
- `reset_ttl_minutes: u64` — 重設密碼連結時效，預設 `30`，上限 `1440`（24 小時）
- `registration_ttl_hours: u64` — 註冊確認連結時效，預設 `24`，上限 `168`（7 天）

`host`/`from`/`public_url` 要嘛全填要嘛全空，半設定會被拒絕。

### `telemetry.*`（全部 fixed at boot）
- `format: "pretty" | "json"`, `filter: string`, `ansi: bool`
