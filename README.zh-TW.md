# tessera

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)

[English](README.md) | **繁體中文**

匿名檔案快遞。上傳一個檔案或一段筆記,拿到一組取件碼,payload 會在過期或取件次數用完之後自我銷毀。

自架、單一執行檔。前端已內嵌其中,不需要在前面架任何東西,也不需要額外跑什麼服務。

## 執行方式

```bash
docker build -f apps/server/Dockerfile -t tessera .
docker run -p 8787:8787 -v ./data:/data tessera
```

image 建置時就會把前端一起編譯進去,所以這就是全部步驟——沒有額外階段,主機上除了 Docker 也不用裝任何東西。

或是從原始碼跑:

```bash
pnpm install
pnpm --filter web build      # SPA 會從 apps/web/dist 內嵌進去
pnpm server:dev              # http://127.0.0.1:8787
```

第一次啟動會建立一個管理員帳號,密碼只會印在 log 裡一次。

## 這裡有什麼

```
apps/server      整個後端,Rust —— API、儲存、認證、內嵌的 SPA
apps/web         使用者用的那個 app,以及 /admin 底下操作者用的後台,React + TanStack Router
apps/tui         終端機用戶端
apps/extension   瀏覽器擴充功能
apps/fumadocs    文件網站
packages/ui      共用的 shadcn/ui 元件
packages/i18n    共用的多語系機制
packages/config  共用的 tsconfig base
```

所有業務邏輯都在後端。前端只透過 API 跟後端溝通,沒有別的管道,所以不管是機器人、CLI 還是 ShareX,能做的事跟瀏覽器完全一樣。

## 契約

repository 根目錄的 `openapi.json` 是由 `utoipa` 從 handler 產生的——這是一個建置產物,不是給人手改的東西。CI 會重新產生一份並比對差異,handler 改了卻沒同步反映到契約裡就無法合併。所有 client 都是從這份檔案產生的,後端本身不會輸出任何 TypeScript。

```bash
pnpm server:openapi                                     # 重新產生
TESSERA_FEATURES__DOCS=true pnpm server:dev             # 在 /docs 瀏覽
```

文件介面預設是關閉的,因為它的 renderer 是從 CDN 拉的,這樣可以讓執行檔少帶大約 11 MB 的內嵌 UI。`openapi.json` 不管開關與否都會提供。

## 設定

幾乎所有東西都可以在後台面板改,下一次請求就生效。有十六項設定不行:socket 已經綁定、連線池已經建好,而且 log subscriber 在兩者之前就已經安裝完成。這些設定來自環境變數,前綴為 `TESSERA_`,巢狀層級用 `__` 分隔:

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=sqlite://data/tessera.db      # 或 postgres:// / mysql://
TESSERA_SERVER__DATA_DIR=data                       # 金鑰檔案,以及第一個 backend
```

檔案放在哪裡不屬於這十六項之一。全新的實例預設把檔案存在 data 目錄下的 `blobs`,可以在面板裡或用 `tessera-server backends add` 加入新的 bucket,不需要重啟。每個 backend 預設都編譯進執行檔,所以要指向 S3 或 Postgres 也不需要重新建置。

### backend 可以指向哪裡

以下每一項都在每次變更時跑過完整測試套件,所以這份清單反映的是「目前確實可用」,而不是「理論上應該可以」:

| 儲存端 | URL | 備註 |
| --- | --- | --- |
| 本機目錄 | `data/blobs` | 預設值,不需要 object store |
| Garage | `s3://bucket` |  |
| RustFS | `s3://bucket` |  |
| MinIO | `s3://bucket` |  |
| SeaweedFS | `s3://bucket` | 需要 `-s3.config` identities 檔案,否則所有寫入都會被拒絕 |
| VersityGW | `s3://bucket` |  |
| s3proxy | `s3://bucket` | 需要新於 2.6.0 的版本——該版本之前沒有 bulk delete |
| Azure Blob | `azure://container` | 對照 Azurite 測試 |
| WebDAV | `https://host/path` | **只適合小檔案**——見下方說明 |

AWS S3、Cloudflare R2 和 Backblaze B2 用的是跟上面那組一樣的 S3 API,但沒有 emulator 能替代它們,所以是靠一個 opt-in job 測試,只有在 operator 加入憑證時才會跑。

選擇之前值得知道的兩個限制:

- **WebDAV** 只接受小於 `share.inline_threshold_bytes`(預設 64 KiB)的檔案。更大的檔案需要分段寫入,但 WebDAV 唯一的上傳動詞是整包 `PUT`,底層的函式庫沒辦法串流處理。檔案太大會直接回 `413`,而不是寫到一半才失敗。
- **Google Cloud Storage** 已編譯進執行檔,但未經測試。沒有任何 emulator 實作這個函式庫寫入時用到的 API,所以這裡沒有任何東西能證明它真的可用。

這個區分就是唯一的規則:**只要在面板裡,改了就會生效。** 面板從來不會顯示「已儲存——現在請重啟」,因為需要重啟的設定會直接顯示為唯讀,並附上原因。

repository 根目錄有兩個範例可以參考——`tessera.personal.env.example` 給只有你自己會連進去的實例,`tessera.public.env.example` 給陌生人上傳檔案用的實例。兩者都先列出環境變數的部分,再列出該情境下值得調整的面板設定,因為這兩種情境要的東西恰好相反。`contrib/tessera.service` 是在容器外執行時用的 systemd unit。

沒有面板的情況——`features.accounts` 關閉的實例沒有 session 可以登入——就只剩命令列:

```bash
tessera-server settings list
tessera-server settings set share.default_expiry_hours 720
tessera-server settings reset-all        # 設錯之後回到預設值的方法
```

其餘內容記載在 `apps/server/README.md`,包含如何把建置縮到能塞進資源受限的機器上。

## 開發

```bash
pnpm dev              # 同時跑所有前端
pnpm server:dev       # 後端
pnpm server:check     # lint、test、docs、audit —— CI 跑的就是這些
pnpm check            # lint 跟 format TypeScript 那一側
```

Rust 是一個純粹的 cargo workspace,刻意沒有納入 Turborepo 的 pipeline。
