<p align="center">
  <img src="https://raw.githubusercontent.com/Limatura/tessera/main/apps/web/public/favicon.svg" alt="Tessera logo" width="104" height="104" />
</p>

<h1 align="center">Tessera</h1>

<h3 align="center">The self-hosted, self-destructing file courier.</h3>

<p align="center"><strong>Send a file. Share a retrieval code. Let time do the cleanup.</strong></p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.zh-TW.md">繁體中文</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/github/license/Limatura/tessera?style=flat-square&label=license&color=e74c3c&logo=gnu&logoColor=white" alt="License: AGPL-3.0" /></a>
  <a href="https://github.com/Limatura/tessera/stargazers"><img src="https://img.shields.io/github/stars/Limatura/tessera?style=flat-square&color=f1c40f&logo=github&label=stars" alt="GitHub stars" /></a>
  <a href="https://github.com/Limatura/tessera/network/members"><img src="https://img.shields.io/github/forks/Limatura/tessera?style=flat-square&color=8e44ad&logo=github&label=forks" alt="GitHub forks" /></a>
  <img src="https://img.shields.io/badge/frontend-React-61DAFB?style=flat-square&logo=react&logoColor=20232A" alt="Frontend: React" />
  <img src="https://img.shields.io/badge/routing-TanStack%20Router-ff4154?style=flat-square&logo=reactrouter&logoColor=white" alt="Routing: TanStack Router" />
  <img src="https://img.shields.io/badge/backend-Rust-f39c12?style=flat-square&logo=rust&logoColor=white" alt="Backend: Rust" />
  <img src="https://img.shields.io/badge/deploy-self--hosted-27ae60?style=flat-square&logo=docker&logoColor=white" alt="Self-hosted" />
  <img src="https://img.shields.io/badge/sharing-ephemeral-8e44ad?style=flat-square&logo=clockify&logoColor=white" alt="Ephemeral sharing" />
  <img src="https://img.shields.io/badge/storage-S3--compatible-16a085?style=flat-square&logo=amazons3&logoColor=white" alt="S3 compatible storage" />
</p>

---

Tessera is an **anonymous file and note sharing service** built for people who want to hand something over without creating another permanent link.

Upload a file or a note, receive a retrieval code, and let the payload expire when its time or retrieval limit runs out. Host it yourself, keep control of your data, and run the service as a **single binary with the frontend embedded**.

## ✨ Why Tessera?

| | |
|---|---|
| 🕶️ **Anonymous sharing** | Send files or notes with a retrieval code, without turning sharing into a permanent account-based workflow. |
| ⏳ **Self-destructing payloads** | Set expiry and retrieval limits so shared content does not need to live forever. |
| 📦 **Storage that fits your setup** | Start with local disk, then add supported S3-compatible, Azure Blob, or other backends. |
| 🧩 **One backend, many clients** | The browser, terminal client, and browser extension use the same API. |
| 🛠️ **Operator console** | Manage instance settings and storage backends from `/admin`. |
| 🪶 **Single-binary deployment** | The frontend is embedded in the Rust server—no separate Node.js runtime or reverse proxy is required by the app itself. |
| 📜 **API-first contract** | OpenAPI is generated from the server handlers and checked in CI. |

## 🚀 Get started

### Docker

Build from the repository root:

```bash
docker build -f apps/server/Dockerfile -t tessera .
docker run --rm -p 8787:8787 -v tessera-data:/data tessera
```

Open **http://localhost:8787**.

The image build compiles the frontend and embeds it into the server. The persistent volume keeps the instance data across container replacement.

> [!IMPORTANT]
> On first startup, Tessera creates an administrator and prints the password **once in the server log**. Save it somewhere secure.

### Run from source

Requirements: Rust toolchain and pnpm.

```bash
pnpm install
pnpm --filter web build
pnpm server:dev
```

Then visit **http://127.0.0.1:8787**.

## 🧱 How it fits together

```text
                    ┌─────────────────────────────┐
                    │         Clients             │
                    │  Web · TUI · Extension      │
                    └──────────────┬──────────────┘
                                   │ HTTP API
                    ┌──────────────▼──────────────┐
                    │       tessera-server        │
                    │  Rust · API · Auth · Admin  │
                    │      Embedded Web SPA       │
                    └─────────┬──────────┬────────┘
                              │          │
                    ┌─────────▼───┐  ┌───▼────────────┐
                    │ SQL database│  │ Payload storage│
                    │ SQLite /    │  │ Local / S3 /   │
                    │ PostgreSQL /│  │ Azure / WebDAV │
                    │ MySQL       │  │                │
                    └─────────────┘  └────────────────┘
```

The server owns the business logic. Clients communicate with it through the API, so integrations do not need to reimplement server-side rules.

## 🗄️ Storage options

Tessera starts with a local `blobs` directory and can add backends at runtime from the admin panel or CLI.

| Backend | Status / notes |
|---|---|
| Local filesystem | Default; no object-storage service required |
| S3-compatible storage | Includes tested integrations such as Garage, RustFS, MinIO, SeaweedFS, VersityGW, and s3proxy |
| Azure Blob Storage | Tested against Azurite |
| WebDAV | Small files only; larger uploads are rejected with `413` |
| AWS S3, Cloudflare R2, Backblaze B2 | S3-compatible; checked by an opt-in job when credentials are configured |
| Google Cloud Storage | Compiled in, but currently unverified by the test suite |

**Storage note:** WebDAV is limited to files below `share.inline_threshold_bytes` (64 KiB by default). Its upload path cannot stream large payloads, so Tessera rejects oversized files instead of letting an upload fail partway through.

## ⚙️ Configuration

Most settings can be changed in the operator console at **`/admin`** and take effect on the next request. Process-level settings—such as the listening port and database connection—must be set through environment variables.

Variables use the `TESSERA_` prefix and `__` for nested keys:

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=sqlite://data/tessera.db
TESSERA_SERVER__DATA_DIR=data
```

The database URL can target SQLite, PostgreSQL, or MySQL. See the example deployment configurations:

- `tessera.personal.env.example` — private, personal instance
- `tessera.public.env.example` — instance intended for public uploads
- `contrib/tessera.service` — systemd service unit

For command-line administration:

```bash
tessera-server settings list
tessera-server settings set share.default_expiry_hours 720
tessera-server settings reset-all
```

## 🧰 Project layout

```text
apps/
├── server/       Rust API, storage, auth, embedded SPA
├── web/          React + TanStack Router application and /admin
├── tui/          Terminal client
├── extension/    Browser extension
└── fumadocs/     Documentation site
packages/
├── ui/           Shared shadcn/ui primitives
├── i18n/         Shared localization utilities
└── config/       Shared TypeScript configuration
crates/
└── tessera-core/ Framework-independent domain logic
openapi.json      Generated API contract
```

The Rust workspace deliberately separates domain rules from the HTTP and persistence layers. The OpenAPI document is generated from handlers; **do not edit it by hand**.

## 🧑‍💻 Development

```bash
pnpm dev              # Run frontend apps in parallel
pnpm server:dev       # Run the Rust server
pnpm server:check     # Server lint, tests, docs, and audit
pnpm check            # TypeScript lint and formatting
pnpm server:openapi   # Regenerate openapi.json
```

Enable the browsable API docs when needed:

```bash
TESSERA_FEATURES__DOCS=true pnpm server:dev
```

The API document is available as `openapi.json` regardless; the interactive docs UI is disabled by default because its renderer is loaded from a CDN.

## 📚 More documentation

- [Server and deployment details](apps/server/README.md)
- [API contract](openapi.json)
- [Personal deployment example](tessera.personal.env.example)
- [Public deployment example](tessera.public.env.example)

## 📄 License

Tessera is licensed under the **GNU Affero General Public License v3.0 (AGPL-3.0)**. See [LICENSE](LICENSE).

<div align="center">

**A little less permanence. A little more control.**

</div>
