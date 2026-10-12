<p align="center">
  <img src="apps/web/public/favicon.svg" alt="" width="88" height="88" />
</p>

<h1 align="center">Tessera</h1>

<p align="center">自架的檔案與文字分享服務，用完會自己清掉。</p>

<p align="center">
  <a href="README.md">English</a> · 繁體中文 · <a href="README.zh-CN.md">简体中文</a> · <a href="README.ja.md">日本語</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Limatura/tessera" alt="License: AGPL-3.0" /></a>
</p>

<p align="center">
  <img src="docs/images/screenshot-upload.png" alt="上傳頁" width="49%" />
  <img src="docs/images/screenshot-share.png" alt="收件人看到的程式碼分享" width="49%" />
</p>

上傳檔案、資料夾、一段程式碼或一個連結，就會拿到一組短短的取件碼和網址。拿到的人可以打開，直到它過期，或下載次數達到你設的上限，之後就會被刪除。

傳東西不需要帳號，除非你把這個功能關掉。Tessera 是單一執行檔，網頁介面直接內嵌在裡面；資料存在 SQLite、PostgreSQL 或 MySQL，檔案放在本機硬碟或物件儲存。

## 適合誰用

**自己和信任的人用。** 架在 NAS 或家用伺服器上、放在 VPN 後面，在自己的裝置之間搬檔案，或傳給朋友。預設值就是為這種情況設計的：開放匿名上傳、SQLite 加本機硬碟，其他都不用設定。參考[個人用的範例設定](tessera.personal.env.example)。

**開放給陌生人的公開站。** 任何人都能上傳，所以這台機器的每一分資源都是陌生人在用。限制單檔大小和總容量、縮短預設的保留時間、註冊維持關閉或改成邀請制，並接上惡意軟體掃描。參考[公開站的範例設定](tessera.public.env.example)。

**團隊或組織。** 關掉匿名上傳，讓大家用公司的 OpenID Connect 登入。用角色控制誰能上傳、誰能看所有人的分享、誰能改設定，每個帳號也可以各自設容量上限。需要不只一台機器時，可以換成 PostgreSQL 和 S3 相容儲存。

## 功能

- **期限與下載次數。** 每個分享都有保留時間，也可以限制最多被取幾次。過期的內容會定期清除。
- **密碼。** 分享可以設密碼，輸入正確才能看到內容或下載。
- **預覽。** 程式碼有語法上色，圖片、SVG 和 Mermaid 圖表直接顯示在頁面上。連結貼到聊天軟體時會有預覽卡片。
- **大檔案。** 上傳中斷後可以續傳，多個檔案可以打包成一個壓縮檔下載。
- **需要時再用帳號。** 註冊可以設成開放、邀請制或關閉。可以用密碼、GitHub、Discord 或任何 OpenID Connect 登入。角色決定誰能做什麼，API token 讓腳本也能上傳。
- **可選的惡意軟體掃描**，透過 ClamAV 或你自己的 HTTP 掃描服務。
- **介面提供英文、繁體中文和日文。**

## 管理後台

<p align="center">
  <img src="docs/images/screenshot-admin.png" alt="後台儀表板" width="80%" />
</p>

管理需要的東西都在 `/admin`，幾乎所有設定改完下一個請求就生效，不用重啟：

- **儀表板：** 用了多少空間、存在哪裡、流量和趨勢，以及有風險的設定提醒。
- **使用者：** 停用或刪除帳號、調整角色、設定個別帳號的容量上限，也能一次結束某個帳號上傳的所有分享。
- **分享：** 站上所有的分享，包含匿名上傳的，以及是誰上傳的。
- **邀請碼、角色與權限。**
- **設定：** 登入方式、上傳限制、預設期限、儲存後端、電子郵件、掃描、連結預覽和外觀。

沒有瀏覽器的話，同樣的設定也能用指令改：`tessera-server settings list`、`settings set <key> <value>`。

## 快速開始

用 Docker，在這個 repo 的 clone 裡執行：

```bash
docker compose up -d
```

然後打開 <http://localhost:8787>。第一次啟動時，伺服器會建立一個管理員帳號，並把密碼印在 log 裡，只印一次：

```bash
docker compose logs tessera | grep password
```

要開放到網路上之前，請先讀[公開站的範例設定](tessera.public.env.example)。匿名上傳預設是開的，沒設容量上限的話，任何連得到這台伺服器的人都能把硬碟塞滿。

### 從原始碼執行

需要 Rust 和 pnpm。

```bash
pnpm install
pnpm --filter web build
pnpm server:dev
```

## 設定

大部分設定都在管理後台。少數必須在伺服器啟動前就決定的，例如連接埠、資料庫網址和資料目錄，用環境變數設定：

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=postgres://tessera:secret@localhost/tessera
TESSERA_SERVER__DATA_DIR=/var/lib/tessera
```

有兩份附註解的範例可以當起點：[個人用](tessera.personal.env.example)和[公開站](tessera.public.env.example)。用 systemd 的話有現成的 [unit 檔](contrib/tessera.service)。

### 儲存

檔案預設放在本機目錄，要用其他儲存後端可以在管理後台新增。

| 儲存後端 | 說明 |
| --- | --- |
| 本機硬碟 | 預設 |
| S3 相容儲存 | 已用 MinIO、Garage、RustFS、SeaweedFS、VersityGW 和 s3proxy 測試。有提供憑證時，也會測 AWS S3、Cloudflare R2 和 Backblaze B2。 |
| Azure Blob Storage | 已用 Azurite 測試 |
| Google Cloud Storage | 已內建，但還沒有測試涵蓋 |
| WebDAV | 只能放小檔案，上限是 `share.inline_threshold_bytes`（預設 64 KiB） |

## 命令列工具

```bash
cargo install --path apps/cli

tessera send report.pdf --server https://share.example.com --expires 24
tessera get https://share.example.com/s/P8D72PSF
echo "hello" | tessera send - --server https://share.example.com
```

加上 `--encrypt` 會在內容離開你的電腦前先加密，金鑰是連結 `#` 後面那段，伺服器永遠看不到。

## 文件

- [使用與架設指南](https://github.com/Limatura/tessera-docs)
- [伺服器內部說明](apps/server/README.md)，以及 [docs/](docs/README.md) 裡的設計筆記
- [API 規格](openapi.json)，由伺服器程式碼產生

## 參與貢獻

請看 [CONTRIBUTING.md](CONTRIBUTING.md)。安全問題請照 [SECURITY.md](SECURITY.md) 私下回報，不要開 issue。

## 授權

[AGPL-3.0](LICENSE)。如果你修改了 Tessera 並提供給其他人使用，必須讓他們能取得修改後的原始碼。
