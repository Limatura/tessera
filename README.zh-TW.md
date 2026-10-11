<div align="center">

# ◈ Tessera

### 自架、閱後即焚的檔案快遞。

**上傳檔案、分享取件碼，剩下的交給時間處理。**

[English](README.md) · [繁體中文](README.zh-TW.md)

[![License: AGPL-3.0](https://img.shields.io/badge/License-AGPL--3.0-e74c3c?style=for-the-badge&logo=gnu&logoColor=white)](LICENSE)
![Backend: Rust](https://img.shields.io/badge/Backend-Rust-f39c12?style=for-the-badge&logo=rust&logoColor=white)
![Self-hosted](https://img.shields.io/badge/Deploy-Self--hosted-27ae60?style=for-the-badge&logo=homeassistant&logoColor=white)
![Ephemeral sharing](https://img.shields.io/badge/Sharing-Ephemeral-8e44ad?style=for-the-badge&logo=clockify&logoColor=white)
![Docker](https://img.shields.io/badge/Deploy-Docker-2496ed?style=for-the-badge&logo=docker&logoColor=white)
![S3 compatible](https://img.shields.io/badge/Storage-S3--compatible-16a085?style=for-the-badge&logo=amazons3&logoColor=white)

</div>

---

Tessera 是一套**匿名檔案與文字分享服務**，適合想快速交付內容、又不想留下永久分享連結的情境。

上傳檔案或筆記，取得取件碼；當內容到期或取件次數用完，就不再需要繼續保留。你可以自行架設、掌握資料存放位置，並以**內嵌前端的單一執行檔**運行服務。

## ✨ 為什麼選 Tessera？

| | |
|---|---|
| 🕶️ **匿名分享** | 透過取件碼交付檔案或筆記，不必把分享流程綁定成永久帳號工作流。 |
| ⏳ **自動到期** | 設定有效期限與取件次數，避免分享內容無限期留存。 |
| 📦 **彈性的儲存方式** | 預設使用本機磁碟，也可加入支援的 S3 相容、Azure Blob 等儲存後端。 |
| 🧩 **多種用戶端，共用 API** | 網頁、終端機用戶端與瀏覽器擴充功能共用同一套 API。 |
| 🛠️ **管理後台** | 在 `/admin` 管理實例設定與儲存後端。 |
| 🪶 **單一執行檔部署** | 前端直接內嵌在 Rust 伺服器中，應用程式本身不需要額外的 Node.js runtime 或反向代理。 |
| 📜 **API 契約優先** | OpenAPI 由伺服器 handler 產生，並由 CI 檢查是否同步更新。 |

## 🚀 快速開始

### 使用 Docker

在 repository 根目錄執行：

```bash
docker build -f apps/server/Dockerfile -t tessera .
docker run --rm -p 8787:8787 -v tessera-data:/data tessera
```

接著開啟 **http://localhost:8787**。

建置 image 時就會編譯前端並內嵌進伺服器。持久化 volume 則用來保留容器替換後仍需存在的資料。

> [!IMPORTANT]
> 第一次啟動時，Tessera 會建立管理員帳號，並且**只在伺服器 log 中顯示一次密碼**。請務必安全保存。

### 從原始碼執行

需求：Rust toolchain 與 pnpm。

```bash
pnpm install
pnpm --filter web build
pnpm server:dev
```

接著前往 **http://127.0.0.1:8787**。

## 🧱 系統架構

```text
                    ┌─────────────────────────────┐
                    │           用戶端            │
                    │   Web · TUI · 瀏覽器擴充    │
                    └──────────────┬──────────────┘
                                   │ HTTP API
                    ┌──────────────▼──────────────┐
                    │       tessera-server        │
                    │   Rust · API · 認證 · 後台  │
                    │          內嵌 SPA           │
                    └─────────┬──────────┬────────┘
                              │          │
                    ┌─────────▼───┐  ┌───▼────────────┐
                    │   SQL 資料庫 │  │    檔案儲存    │
                    │ SQLite /    │  │ 本機 / S3 /    │
                    │ PostgreSQL /│  │ Azure / WebDAV │
                    │ MySQL       │  │                │
                    └─────────────┘  └────────────────┘
```

所有業務邏輯都由後端負責。用戶端透過 API 與伺服器溝通，因此整合工具不需要自行重寫伺服器端的規則。

## 🗄️ 支援的儲存後端

Tessera 預設使用本機的 `blobs` 目錄，也可以透過管理後台或 CLI 在執行期間新增後端。

| 儲存後端 | 狀態／備註 |
|---|---|
| 本機檔案系統 | 預設選項，不需要額外的物件儲存服務 |
| S3 相容儲存 | 包含已測試的 Garage、RustFS、MinIO、SeaweedFS、VersityGW 與 s3proxy |
| Azure Blob Storage | 以 Azurite 進行測試 |
| WebDAV | 僅適用於小檔案；較大的上傳會被拒絕並回傳 `413` |
| AWS S3、Cloudflare R2、Backblaze B2 | 使用 S3 相容 API；設定憑證後由 opt-in 工作流程測試 |
| Google Cloud Storage | 已編譯進執行檔，但目前尚未由測試套件驗證 |

**WebDAV 限制：** WebDAV 只接受小於 `share.inline_threshold_bytes` 的檔案，預設為 64 KiB。由於其上傳路徑無法串流處理大型 payload，Tessera 會直接拒絕過大的檔案，避免上傳到一半才失敗。

## ⚙️ 設定方式

大部分設定都能在 **`/admin` 管理後台**修改，並於下一次請求生效。像是監聽 port、資料庫連線等程序啟動後無法變更的設定，則必須透過環境變數指定。

環境變數使用 `TESSERA_` 前綴，巢狀層級以 `__` 分隔：

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=sqlite://data/tessera.db
TESSERA_SERVER__DATA_DIR=data
```

資料庫支援 SQLite、PostgreSQL 與 MySQL。部署範例：

- `tessera.personal.env.example` — 僅供自己使用的私人實例
- `tessera.public.env.example` — 開放陌生人上傳的公開實例
- `contrib/tessera.service` — systemd service unit

也可以透過 CLI 管理設定：

```bash
tessera-server settings list
tessera-server settings set share.default_expiry_hours 720
tessera-server settings reset-all
```

## 🧰 專案結構

```text
apps/
├── server/       Rust API、儲存、認證與內嵌 SPA
├── web/          React + TanStack Router 應用程式與 /admin
├── tui/          終端機用戶端
├── extension/    瀏覽器擴充功能
└── fumadocs/     文件網站
packages/
├── ui/           共用 shadcn/ui 元件
├── i18n/         共用多語系工具
└── config/       共用 TypeScript 設定
crates/
└── tessera-core/ 與框架無關的領域邏輯
openapi.json      自動產生的 API 契約
```

Rust workspace 將領域規則與 HTTP、資料持久化層刻意分離。根目錄的 `openapi.json` 由 handler 產生，**請勿手動修改**。

## 🧑‍💻 開發

```bash
pnpm dev              # 平行啟動所有前端應用程式
pnpm server:dev       # 啟動 Rust 伺服器
pnpm server:check     # 執行伺服器 lint、測試、文件與 audit
pnpm check            # TypeScript lint 與格式檢查
pnpm server:openapi   # 重新產生 openapi.json
```

需要互動式 API 文件時：

```bash
TESSERA_FEATURES__DOCS=true pnpm server:dev
```

無論是否啟用互動式文件介面，`openapi.json` 都會提供。互動式文件 UI 預設關閉，因為它的 renderer 會從 CDN 載入。

## 📚 延伸文件

- [伺服器與部署細節](apps/server/README.md)
- [API 契約](openapi.json)
- [私人部署範例](tessera.personal.env.example)
- [公開部署範例](tessera.public.env.example)

## 📄 授權條款

Tessera 採用 **GNU Affero General Public License v3.0（AGPL-3.0）**。詳見 [LICENSE](LICENSE)。

<div align="center">

**少一點永久留存，多一點自主掌控。**

</div>
