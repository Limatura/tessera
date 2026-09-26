# 設定 API 可設定的參數

來源：[config.rs](src/config.rs)、[settings.rs](src/settings.rs)、[api/settings.rs](src/api/settings.rs)、[crates/tessera-core](../../crates/tessera-core/src/)

> 這份清單以 `settings::keys()` 的實際輸出為準（key 由 `Config::default()` 序列化後走訪產生）。
> 新增 config 欄位時記得同步更新這裡。

## API

- `GET /admin/settings` — 列出所有設定（含不可改的，標 `editable: false` + `refusal` 原因）
- `PUT /admin/settings` — 改設定，`changes: {key: value}`，`value: null` 清除回退到環境變數/預設值
- `POST /admin/settings/preview` — 預覽改動會刪掉什麼（如縮短保留天數），不會真的套用

改設定需要 `SettingsWrite` 權限（`preview`/`GET` 用 `SettingsRead`），且要 `features.admin` 有開。

**整批驗證**：`PUT` 的 changes 是一起驗證的，一個 key 不合法會讓整批退回，所以只送真正改動的欄位。

## 三種不可改的理由

| 標記 | 意思 | 面板行為 |
|---|---|---|
| `FIXED_AT_BOOT` | 開機時就決定了，改了也不會生效 | 讀得到，寫入被拒 |
| `SECRET` | 帶憑證，不該進 settings 表 | 讀不到也寫不了，只能用環境變數 |
| `SEALED` | 帶憑證但 operator 有理由從面板設定 | **寫得進去，讀不回來**（加密儲存） |

### `FIXED_AT_BOOT`

| Key | 原因 |
|---|---|
| `server.host` / `server.port` | listener 已綁定 |
| `database.*` | 讀 settings table 本身要先有連線池 |
| `telemetry.*` | log subscriber 早於一切被安裝 |
| `server.cors_allowed_origins` | 建路由時就烤進 middleware |
| `server.max_body_bytes` | 同上，且只有一半邏輯會跟著改（見下方說明） |
| `server.assets_dir` | SPA 開機時讀一次就內嵌了 |
| `server.data_dir` | 金鑰檔跟第一個 backend 在首次啟動時放在這，事後搬家會把兩個都留在原地 |
| `security.secret_key` | 建立 sealer 用，換掉會導致所有加密存的設定開不了 |
| `features.docs` | 建路由時決定要不要掛 |

### `SECRET`

| Key | 原因 |
|---|---|
| `auth.admin_password` | 只在首次啟動用一次，存起來沒意義 |
| `database.url` | URL 裡帶帳密 |
| `security.secret_key` | 能解開所有加密存的設定 |

### `SEALED`

| Key |
|---|
| `auth.oidc.client_secret` |
| `auth.github.client_secret` |
| `auth.discord.client_secret` |
| `mail.username` / `mail.password` |
| `scan.authorization` |

面板可以寫入，也會被告知「有沒有設過」，但拿不到值。前端表單要做成「**留空 = 不改動，有填 = 覆寫**」，不能用常規的載入現值再編輯。

儲存時用 `security.secret_key` 加密；沒設的話用首次啟動寫在 `server.data_dir/secret.key` 的那把，所以一定寫得進去。

---

## `appearance.*`

signed-out 訪客看到的樣子，以及登入者沒自己選過的部分。會寫進 `index.html`，所以首次繪製就是對的顏色。

- `mode: "light" | "dark" | "system"` — 預設 `system`（跟隨 `prefers-color-scheme`）
- `theme: "default" | "graphite" | "sepia"` — 中性色盤（背景/文字/邊框）。graphite 較冷偏平，sepia 暖如紙。預設 `default`
- `accent: "violet" | "rose" | "orange" | "amber" | "green" | "teal" | "blue"` — 強調色（按鈕/連結/focus ring），預設 `violet`

拆成三個 key 而非一個物件：面板的 key 清單是從結構讀出來的，巢狀物件會被當成「一組設定」而非一個設定。好處是值由 parse 驗證，未知的 accent 跟未知的任何東西走同一條拒絕路徑。

## `auth.*`

### 基本

- `registration: "open" | "invite" | "closed"` — 誰能註冊，預設 `open`
- `default_role: string` — 新帳號的角色，**用名稱不是 id**，預設 `member`。啟動時檢查角色存在，不存在就開不起來
- `session_ttl_hours: u64` — 預設 `720`（30 天）
- `login_history_days: u64` — 已結束的 session 在登入紀錄留幾天，預設 `90`。用途是回答「有沒有別人進來過」，所以活得比憑證久
- `cookie_secure: "auto" | "always" | "never"` — 何時給 session cookie 標 `Secure`，預設 `auto`
  - `auto` 走 HTTPS 才標，同時相容 LAN 明文與公開 TLS 兩種部署
  - `always` 在非全 HTTPS 環境會**讓登入靜默失效**（瀏覽器收下回應、丟掉 cookie，使用者看到登入成功但什麼都沒發生）
- `admin_username` / `admin_email: string` — 首次啟動建立的管理員資訊
- `admin_password: string`（SECRET）— 空 = 自動產生並印出一次

### `auth.password.*`

長度是主要規則，組成規則預設全關。依 NIST 800-63B：那些規則只會逼出 `Password1!`，長度比字元集更有價值。留著它們是因為有些營運者要應付還在要求組成規則的內部政策。

- `min_length: usize` — 預設 `8`，下限 8
- `max_length: usize` — 預設 `256`，上限 1024。需要上限是因為無上限的密碼欄位等於「未認證請求可叫伺服器做任意多的工」
- `require_mixed_case: bool` — 要同時有大寫和小寫字母，預設 `false`
- `require_digit: bool` — 要有數字，預設 `false`
- `require_symbol: bool` — 要有符號，預設 `false`。**空白不算符號**，否則任何 passphrase 都自動過關

組成規則只管「**選新密碼**」（註冊、改密碼、重設）。登入不檢查，所以開了規則不會把既有帳號鎖在外面。字元類別照 Unicode 判斷（見 `PasswordPolicy` 的註解表，含前端對應的 regex），任何語言的密碼都用同一套標準。

### `auth.pwned.*`（外洩密碼檢查，預設關）

- `enabled: bool` — 設新密碼時查 Have I Been Pwned，預設 `false`
- `url: string` — range API 的位址，預設 `https://api.pwnedpasswords.com`。可以指到自架的實作

前端可以邊打字邊查：`GET /api/v1/auth/pwned/{prefix}` 帶 SHA-1 前 5 個字，回該前綴下真正外洩過的後綴（padding 已拿掉），比對在瀏覽器做。`/capabilities` 的 `account_password_breach_check` 說這台有沒有開。外部服務查不到時回 503，送出時的檢查照樣會跑。

送出去的只有密碼 SHA-1 的前 5 個字，比對在本機做（k-anonymity），完整密碼和完整雜湊都不會離開這台。查不到服務（逾時、連不上、非 200）就放行，只在 log 留一行——註冊、改密碼、重設密碼都不能因為別人的服務掛掉而停擺。註冊、改密碼、重設密碼三條路都會查。

### `auth.hashing.*`（Argon2id）

- `memory_kib: u32` — 預設 `19456`（19 MiB，OWASP 建議值）。**最關鍵的參數**：記憶體成本是 GPU 破解變貴的原因，也等於登入尖峰的記憶體用量 × 同時執行數
- `iterations: u32` — 預設 `2`
- `lanes: u32` — 預設 `1`

改這些**不會讓既有密碼失效**：參數寫在雜湊值本身裡，驗證時從那讀，所以只影響新產生的雜湊。

### `auth.throttle.*`

八組計數器保護不同東西，每組都是 `{max_attempts, window_secs}`。

| Key | 計數依據 | 保護什麼 |
|---|---|---|
| `login` | 帳號 + 來源 | 失敗的登入嘗試 |
| `source` | 來源位址 | 任何昂貴操作。**刻意寬鬆**——位址是共用的，擋一個等於擋掉它後面所有人 |
| `lookup` | 來源 | 查了不存在的取件碼。**短取件碼的安全性完全靠這個** |
| `reset` | 被查詢的 email | 重設密碼信的發送 |
| `registration` | 被查詢的 email | 註冊確認信的發送 |
| `verification` | 收信的 email | 驗證 email 的信。**不共用 `reset`**，否則把自己的 email 改成別人的就能耗掉對方重設密碼的額度 |
| `share` | 取件碼 + 來源 | 分享密碼猜錯次數 |
| `speed` | 來源 | 測速。算的是**每次測速**而不是失敗次數，限的是一個位址一小時能用掉多少頻寬 |
| `pwned` | 來源 | 打字時查外洩密碼（`GET /auth/pwned/{prefix}`）。每次查詢都算，預設每分鐘 30 次。**不共用 `source`**，打密碼不會吃掉同位址的登入額度 |

**兩個值都不能是 0**，啟動時會拒絕：0 次嘗試等於鎖死所有人（在 `source` 上就是整台機器），0 秒視窗則永遠不會擋，看起來開著實際上關著。要放寬就調高 `max_attempts`。

- `max_tracked_keys: usize` — 清掃前最多追蹤幾個 key，上限 `100_000`。**這是記憶體上限不是安全設定**：每個 key 都是未認證陌生人自己選的字串，這個數字決定他們能讓 process 佔多少記憶體。八組共用

### `auth.oidc.*`

空的 `issuer` = 整個功能關閉，不掛任何路由、不做任何宣告。

Provider 決定**誰能登入**，tessera 決定**他能做什麼**（新帳號落在 `auth.default_role`，之後由管理員調整，跟密碼註冊的帳號一樣）。group claim 只用於 `required_group`，**不會映射到角色**。

- `issuer: string` — provider 的 issuer URL，其餘設定全由它 discovery 取得。必須 https，且與 `iss` claim 完全字串相等（規範要求）
- `client_id: string`
- `client_secret: string`（SEALED）
- `scopes: string[]` — `openid` 缺了會自動補；**`email` 是必要的**，沒有 email 的登入會被拒絕而不是把欄位改成 nullable
- `required_group: string` — **只允許這個群組的人登入。空字串 = provider 認證過的任何人都能進**
- `group_claim: string` — 群組在哪個 claim。Authentik/Authelia 用 `groups`，Keycloak 用 `realm_access.roles`，部分 LDAP bridge 用 `memberOf`。點號選巢狀欄位
- `display_name: string` — 登入按鈕文字，空則用 issuer 的 host
- `link_by_email: "never" | "when_verified" | "always"` — provider 身分能否綁到同 email 的既有帳號，預設 `never`
  - `when_verified`：provider 要說 `email_verified` 是 true 才綁
  - `always`：不管 provider 怎麼說都綁。給 Synology、Entra ID、Authentik 這類不送或亂送 `email_verified` 的 provider 用。**provider 若讓任何人填任何信箱註冊，選這個等於讓人靠填別人的信箱接管這裡的帳號**，只適合自己掌控帳號的 IdP
- `allow_registration: "inherit" | "always" | "never"` — 第一次從這家登入時要不要開帳號，預設 `inherit`（跟著 `auth.registration`，含邀請碼）。`always` 不看 `auth.registration`、也不用邀請碼，給「帳號全在自己 IdP 管、這邊關掉密碼註冊」的站台；`never` 只讓已存在的帳號登入

> **`required_group` 留空是實質的安全問題。**
> Provider 服務多個應用時，它認證的是「所有在它那裡有帳號的人」，不等於「該進你這個站的人」。
> 只設了 Google 又留空 `required_group`，結果是全世界任何 Google 帳號都能登入。

### `auth.github.*` / `auth.discord.*`

兩組欄位結構相同。與 OIDC 分開是因為這兩家不發佈 discovery 文件，端點寫死在 adapter 裡，這裡只剩「向他們表明本站身分」的那對值。

- `client_id: string`
- `client_secret: string`（SEALED）
- `scopes: string[]` — **不是裝飾**：GitHub 沒有 `user:email` 不給已驗證信箱，Discord 沒有 `email` 也不給。改窄了會讓每次登入都 `no_address` 失敗。預設已是可運作組合，啟動時驗證
- `base_url: string` — 給 GitHub Enterprise Server 用（自架在自己網域下）。空 = github.com。**Discord 無視此欄位**
- `display_name: string` — 按鈕文字，空則用 provider 名稱
- `link_by_verified_email: bool` — 預設 `false`。理由同 OIDC：這兩家帳號一分鐘就能註冊，同時開放註冊又用 email 比對等於把別人的帳號送出去
- `allow_registration: "inherit" | "always" | "never"` — 同 OIDC。**這兩家沒有群組可以篩，`always` 等於全世界任何 GitHub／Discord 帳號都能在這台開戶**

## `share.*`

- `code_length: usize` — 取件碼字數，預設 `8`。8 個 Crockford base32 ≈ 1.1×10¹² 組合；某些同類服務用的 5 碼只有 3300 萬，公開端點可被走訪完。**短碼只有在 `throttle.lookup` 存在時才站得住腳**
- `inline_threshold_bytes: usize` — 小於此值存資料庫而非物件儲存，預設 `64 KiB`。個人實例多數分享是短筆記或程式片段，跑一趟物件儲存的成本高於省下的空間；順帶讓資料庫備份也涵蓋內容
- `max_text_bytes: usize` — 純文字分享上限，預設 `1 MiB`。文字存資料庫，所以這是**單一 row 的最大值，也是一次上傳能佔的最大記憶體**——整包收下的東西沒辦法串流
- `ticket_ttl_hours: u64` — 下載授權存活上限，預設 `24`。一次授權消耗一次取件次數，之後產生的 ticket 可重複使用（Range 請求、續傳、斷線重試）；**沒有這層分離，一次中斷的下載就會燒掉一個一次性分享**。這是天花板不是常態，多數 ticket 先死於 idle
- `ticket_idle_hours: u64` — ticket 閒置多久失效，預設 `1`。用閒置而非固定壽命：ticket 是一串字所以可轉發，固定壽命必須涵蓋「最慢的線下載最大的檔」，而活那麼久的 ticket 就值得被轉發了。每次使用都往後推
- `default_expiry_hours: u64` — 未指定時的存活時數，預設 `168`（7 天）。`0` = 用 `max_expiry_hours`。**這比任何配額都更決定磁碟用量**：磁碟上的量約等於上傳速率 × 保留時間
- `max_expiry_hours: u64` — 可指定的最長存活，預設 `720`（30 天）。`0` = 無上限
- `retain_after_death_hours: u64` — 過期/用完後保留多久才真刪，預設 `168`（7 天）。留著是為了讓連結能說「這個過期了」而不是「查無此碼」——只有前者值得重打一次。`0` = 下次清掃就刪
- `upload_session_ttl_hours: u64` — 未完成的上傳留多久，預設 `24`。**期間它佔的位元組一直算在配額裡**。夠久讓闔上的筆電隔夜還能續傳，夠短讓死掉的 session 不堆積

> **用 S3 的話另外設 lifecycle rule。**
> 清掃只會中止它找得到的未完成上傳，失敗的中止會留下分片，而 S3 自己永遠不會清理被遺棄的 multipart upload。
> 設一條 `AbortIncompleteMultipartUpload`、天數比 `upload_session_ttl_hours` 長幾天。

### `share.access_log.*`

- `enabled: bool` — 是否逐筆記錄取件，預設 `false`
- `retention_days: u64` — 保留天數，預設 `30`。**開啟時不可為 0**

預設關是刻意的：`view_count` 是個整數，不說明訪客在哪；而一筆存取紀錄是一個位址加一串瀏覽器字串，記的是「收到連結、什麼都沒答應過」的人。這是 operator 該做的決定，不是預設繼承的東西，而開啟的實例應該在頁面上說明。

`retention_days` 不接受 0：把 0 解讀為「永久」對一個關於別人資料的設定是錯的預設。

`GET /admin/shares/{id}/access-log` 的回應會在 `meta` 帶上 `enabled`，前端才能區分「真的沒人下載」和「根本沒在記錄」——兩者的 `data` 都是空陣列。

## `site.*`

- `title: string` — 分頁標題，也是連結預覽裡的站名，預設 `Tessera`。不可空白，最多 100 字
- `description: string` — 連到這個站本身時，預覽顯示的說明，預設空（空 = 不輸出這個 meta）。最多 300 字

兩個都會在 `index.html` 送出前寫進去，跟 `appearance.*` 一樣，第一次繪製就是對的，不用等 script。分享頁的說明用分享自己的，不會被這個蓋掉。

**不做 SEO 相關欄位**（`keywords` 之類），理由見那張刻意不實作的決策記錄。

## `speed.*`（測速，預設關）

- `enabled: bool` — 有沒有測速端點，預設 `false`
- `anonymous: bool` — 沒登入的人能不能測，預設 `false`（只有登入者）
- `max_bytes: u64` — 單次上限（上行下行各自），預設 25 MiB

端點是 `GET /api/v1/speed/download?bytes=N` 和 `POST /api/v1/speed/upload`，送的位元組沒有意義、收到的直接丟掉。關掉或不開放匿名時，該看不到的人拿到 404，跟沒有這個功能的站一樣。

**這是水龍頭**，所以三道限制都在：預設關、預設只給登入者、單次有上限，再加上 `auth.throttle.speed` 的每來源次數限制。`/capabilities` 的 `speed_test` 會告訴前端這個呼叫者能不能測。

## `security.*`

- `secret_key: string`（SECRET + FIXED）— 封裝所有「存在資料庫、又要讀得回來」的設定，64 個十六進位字元。空（預設）= 用首次啟動自動產生、存在 `server.data_dir/secret.key`（權限 0600）的那把。想讓金鑰不跟資料目錄的備份放一起就自己設（`openssl rand -hex 32`）

它封的是上面 `SEALED` 那張表列的 key：OIDC / GitHub / Discord 的 client secret、mail 帳密、掃毒服務的 authorization、還有從後台加的 storage backend 憑證。**跟物件儲存無關**——以前叫 `storage.secret_key`，是因為 backend 憑證是第一個有這個需求的東西，名字誤導人，已經改掉。

金鑰只能從環境變數來，這樣資料庫備份裡不會同時有鎖和鑰匙。弄丟了就是所有加密存的設定都要重填。

## `scan.*`

惡意軟體掃描，預設關。詳見 `docs/scanning.md`。

- `url: string` — 掃描服務，空字串 = 不掃。`clamd://host:3310`（ClamAV daemon）或 `http(s)://…`（POST 檔案內容，回 `{"clean": bool, "threat": "…"}`）
- `authorization: string`（SEALED）— 送給 HTTP 掃描服務的 `Authorization`

掃完之前分享不能下載（`423` + `Retry-After`）。掃描服務連不上時先放行、恢復後補掃；掃到有毒就停用分享。加密分享和超過 clamd 大小上限的檔案不掃。

## `storage.*`

檔案存哪裡不是設定：每個 backend 都是 `storage_backends` 表裡的一列，從後台 `/admin/storage/backends` 或 `tessera-server backends` 管。全新的實例啟動時自動寫一列 `default`（本機目錄 `server.data_dir/blobs`，預設收新上傳），之後跟其他列一樣可以改、退役、刪除。

- `max_total_bytes: u64` — 整個實例的內容總量上限，`0` = 無限制，預設 `0`
- `split_across_backends: bool` — 單一檔案能否跨多個 backend，預設 `false`
- `reconcile_interval_hours: u64` — 多久對帳一次（物件儲存 vs 資料庫），預設 `24`，`0` 關閉
- `reconcile_grace_hours: u64` — 無主物件要多舊才會被對帳刪掉，預設 `24`

> **`max_total_bytes` 沒設 + `anonymous_upload` 開著（兩者都是預設）= 對所有能連到的人開放的無限網路硬碟**，
> 而第一個徵兆是磁碟滿了寫入開始失敗。
> 併發下是近似值，可能超線最多「同時上傳數 × `server.max_body_bytes`」，留跟磁碟一樣的餘裕。
> 通常 `share.default_expiry_hours` 是更有效的槓桿：縮短它能降低穩態用量而不用拒絕任何人。

`split_across_backends` 開了可以放「比任何單一帳號都大」的檔（五個免費 10GB bucket 裝得下 40GB 檔，關著則回 `507`）。代價是**拆開的檔沒有直連**——presigned URL 只能指一個物件，所以位元組得經過本伺服器或在瀏覽器縫合，免費方案上這同時吃上傳和下載頻寬。只有在沒有任何 backend 能整個裝下時才會拆，見 `docs/splitting.md`。

對帳跟每小時的清掃分開，因為對帳要列出整個 store，而清掃只是對索引欄位下 DELETE。**多數物件儲存把 listing 當 API 呼叫另外計費**——交易預算比位元組預算緊就拉長 `reconcile_interval_hours`，本機磁碟就縮短。

`reconcile_grace_hours` 的寬限刻意給得大方：物件永遠先寫、row 後建，所以每個上傳都有一瞬間看起來像孤兒。晚一天回收漏掉的位元組沒成本，在 row 落地前一刻刪檔則是直接損失檔案。

## `server.*`

- `request_timeout_secs: u64` — 一般請求的時鐘預算，預設 `30`。**不套用在上傳路由**
- `upload_stall_secs: u64` — 上傳多久沒有位元組進來算斷線，預設 `30`
- `part_size_bytes: usize` — 累積多少才送一個 part 到物件儲存，預設 `8 MiB`
- `public_url: string` — 本站對外的絕對 URL，預設空
- `trusted_proxies: string[]` — 可信任的前置代理，CIDR 或裸位址，預設 `[]`
- `trusted_proxy: bool` — **已棄用**，用 `trusted_proxies`。預設 `false`
- `host: IpAddr` / `port: u16`（FIXED）
- `max_body_bytes: usize`（FIXED）— 最大請求 body
- `cors_allowed_origins: string[]`（FIXED）— 空 = 僅同源，單一 binary 部署的正確預設
- `assets_dir: string`（FIXED）— 從磁碟而非內嵌 bundle 提供前端。指向沒有 `index.html` 的目錄會直接讓伺服器停止（而非靜默 fallback 到舊 build）
- `data_dir: string`（FIXED）— 實例自己寫的東西放哪，預設 `data`。只在首次啟動用到：沒設 `security.secret_key` 時的 `secret.key`，以及表是空的時候那個 `blobs` backend

**上傳為什麼不給時鐘預算**：一個數字沒辦法同時滿足兩邊。5GB 檔走 20Mbit/s 要 2400 秒，而那也會是「連上來什麼都不送」的人佔住位置的時間，兩者差三個數量級。所以 `upload_stall_secs` 改問另一個問題：對面還在送嗎？

**`part_size_bytes` 的地雷**：S3 拒絕最後一片以外小於 5 MiB 的 part；Cloudflare R2 還要求每片大小**相同**（所以 R2 上這個值應該等於最小值，如同 tusd 的做法）。Garage 兩者都不檢查，所以 CI 抓不到這裡的錯。

**`public_url` 只有 link preview 需要，而且必須絕對**——`og:image` 是別台機器上的爬蟲來抓的。空則從每個請求的 `Host` 和 forwarded scheme 推導，而這在它針對的場景會壞掉：會改寫 `Host` 的反向代理，或只看得到 `localhost:8080` 的容器。**預覽卡片一片空白就是要設這個。** 結尾斜線會被去掉。

**`trusted_proxies` 預設什麼都不信**：`X-Forwarded-For` / `-Proto` 是最後跟我們講話的人設的，直連時就是呼叫者本人。無條件相信等於讓呼叫者自選來源位址（這裡每個 throttle 都以它計數）並在明文連線上宣稱 HTTPS。同機代理填 `["127.0.0.1", "::1"]`，容器網路填 `["10.0.0.0/8"]`。

已棄用的 `trusted_proxy: true` 意思是「連到我們的都是代理」，所以任何能直接打到這個 port 的人都能偽造標頭。保留是為了升級不會把 operator 鎖在門外，啟動時會警告。

> **`server.max_body_bytes` 是三個 FIXED 裡最糟的一個。**
> 它有一半*會*跟著改——上傳路徑裡手寫的檢查讀的是當下設定，而 `DefaultBodyLimit` layer 不會。
> operator 調高之後會發現大檔還是被一個數字已經對不上的 layer 擋掉。

## `features.*`

執行期開關。**關掉的功能路由根本不會掛上**，所以呼叫它得到單純的 404，而不是一個等於確認「它存在」的 403。關掉不會讓 binary 變小，那是 cargo feature（`postgres`、`storage-s3`）的事。

`GET /api/v1/capabilities` 回報實際生效的結果——SPA 沒辦法從靜態契約看出某個實例開了什麼。

- `accounts: bool` — 註冊、登入、session，預設 `true`。關掉 = 完全沒有帳號，適合「一個人在 VPN 後面只要一個檔案投放點」
- `admin: bool` — 管理端點，也就是 `apps/admin` 整個前端靠的東西，預設 `true`
- `anonymous_upload: bool` — 沒 session 能不能上傳，預設 `true`。**刻意與 `accounts` 獨立**：想要管理面板不等於想強迫所有人註冊
- `encryption: bool` — 允許端對端加密的分享，預設 `false`。關掉只擋新的，已經加密的分享照樣能下載
- `docs: bool`（FIXED）— `/docs` 瀏覽介面，預設 `false`。頁面只是幾行 HTML 從 CDN 拉 renderer，**伺服器自己從不抓它**，是瀏覽器抓。這樣省下約 11MB binary（23.8MB → 12.5MB）。`openapi.json` 無論如何都會提供

## `mail.*`

空的 `host` = 完全關閉，這是預設值也是常見情況。單人實例可以在機器上直接跑 `reset-password` 回到忘記的帳號。

不自己當 mail server 是因為家用線路送不出信：多數 ISP 封鎖 port 25，收信端也不管 SPF/DKIM 直接拒絕住宅位址。這裡預期的是 relay（Resend、Brevo、SES、Fastmail 或公司自己的）。

- `host: string` — SMTP 主機，空字串關閉所有寄信功能
- `port: u16` — 預設 `587`（submission + STARTTLS）；`465` 是隱式 TLS，也常見
- `username: string`（SEALED）— Resend 要求這裡填字面字串 `resend`
- `password: string`（SEALED）— 多數 relay 這裡放的是 API key
- `from: string` — `From:` 位址，網域必須在 relay 驗證過
- `public_url: string` — 信件內連結的基底
- `reset_ttl_minutes: u64` — 重設密碼連結有效分鐘數，預設 `30`，上限 `1440`（24 小時）
- `registration_ttl_hours: u64` — 註冊確認連結時效，預設 `24`，上限 `168`（7 天）。驗證 email 的連結也用這個

`host` / `from` / `public_url` 要嘛全填要嘛全空，半設定會被拒絕。

**`mail.public_url` 刻意用設定值而非請求的 `Host` header**：用 attacker 可控的 header 組出的重設連結會指向他的伺服器，等於把 token 送給他。

兩個時效差很多是因為它們給的權限不同。重設連結**本身就是憑證**，所以短，夠久撐過郵件佇列即可。註冊連結給的少：點了之後建立的帳號用的是申請者自己早就選好的密碼，連結被偷也登不進去。

## `og.*`

- `layout: string` — operator 自訂的預覽卡片 HTML，預設空（用內建卡片）

設了就取代**所有分享**的卡片。但每個分享能露出什麼仍由別處決定，所以鎖定分享的卡片裡 `{{size}}` 會是空的——因為值是空的，不是因為 layout 有做防護。

**只允許 `div`、`span`、`style`，且不能指向本伺服器以外的資源**，違反直接拒絕。見 `og::layout` 和 `docs/link-previews.md`。

## `database.*`（全部 FIXED，僅能用環境變數）

- `url`（SECRET）、`max_connections`（預設 `8`）、`acquire_timeout_secs`（預設 `10`）、`run_migrations`（預設 `true`）

## `telemetry.*`（全部 FIXED）

- `format: "pretty" | "json"` — `pretty` 人類可讀多行，給本機開發；`json` 每行一個物件，給 log shipper
- `filter: string`
- `ansi: bool`
