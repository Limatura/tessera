# tessera

[![License: AGPL v3](https://img.shields.io/badge/License-AGPL_v3-blue.svg)](LICENSE)

**English** | [繁體中文](README.zh-TW.md)

Anonymous file courier. Upload a file or a note, hand out a retrieval code, and the payload destroys itself once it expires or runs out of retrievals.

Self-hosted, one binary. The frontend is embedded in it, so there is nothing to put in front of it and nothing to run beside it.

## Running it

```bash
docker build -f apps/server/Dockerfile -t tessera .
docker run -p 8787:8787 -v ./data:/data tessera
```

The image build compiles the frontend itself, so that is the whole of it — there is no separate step and nothing to install on the host beyond Docker.

Or from source:

```bash
pnpm install
pnpm --filter web build      # the SPA is embedded from apps/web/dist
pnpm server:dev              # http://127.0.0.1:8787
```

The first run creates an administrator and prints its password once, to the log.

## What is in here

```
apps/server      the whole backend, Rust — API, storage, auth, the embedded SPA
apps/web         the app people use and the operator's console at /admin, React + TanStack Router
apps/tui         a terminal client
apps/extension   a browser extension
apps/fumadocs    the documentation site
packages/ui      shared shadcn/ui primitives
packages/i18n    shared locale plumbing
packages/config  the shared tsconfig base
```

The backend holds all of the business logic. The frontend talks to it over the API and nothing else, so a bot, a CLI or ShareX can do exactly what the browser can.

## The contract

`openapi.json` at the repository root is generated from the handlers by `utoipa` — it is a build artefact, not something anybody edits. CI regenerates it and fails on any difference, so a handler change that is not reflected in the contract cannot merge. Clients are generated from that file; the server emits no TypeScript.

```bash
pnpm server:openapi                                     # regenerate it
TESSERA_FEATURES__DOCS=true pnpm server:dev             # browsable at /docs
```

The docs UI is off by default because it pulls its renderer from a CDN, which keeps about 11 MB of bundled UI out of the binary. `openapi.json` is served either way.

## Configuration

Almost everything is changed in the admin panel and takes effect on the next request. Sixteen settings cannot be: the socket is already bound, the connection pool is already built, and the log subscriber was installed before either. Those come from the environment, prefixed `TESSERA_`, with `__` separating nesting levels:

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=sqlite://data/tessera.db      # or postgres:// / mysql://
TESSERA_SERVER__DATA_DIR=data                       # the key file, and the first backend
```

Where files go is not one of them. A fresh instance keeps them in `blobs` under the data directory, and buckets are added from the panel or with `tessera-server backends add`, with no restart. Every backend is compiled in by default, so pointing at S3 or Postgres needs no rebuild.

### What a backend can point at

Each of these has the test suite run against it on every change, so the list is what is currently true rather than what ought to work:

| Store | URL | Notes |
| --- | --- | --- |
| A local directory | `data/blobs` | The default. No object store needed. |
| Garage | `s3://bucket` |  |
| RustFS | `s3://bucket` |  |
| MinIO | `s3://bucket` |  |
| SeaweedFS | `s3://bucket` | Needs an `-s3.config` identities file, or it refuses every write. |
| VersityGW | `s3://bucket` |  |
| s3proxy | `s3://bucket` | Needs a build newer than 2.6.0 — that release has no bulk delete. |
| Azure Blob | `azure://container` | Checked against Azurite. |
| WebDAV | `https://host/path` | **Small files only** — see below. |

AWS S3, Cloudflare R2 and Backblaze B2 speak the same S3 API as the first group and no emulator can stand in for them, so they are checked by an opt-in job that runs only when an operator adds credentials.

Two limits worth knowing before choosing:

- **WebDAV** takes only files below `share.inline_threshold_bytes` (64 KiB by default). Anything larger is written a piece at a time, WebDAV's only upload verb is a whole-body `PUT`, and the library underneath cannot stream one. A file too large is refused with `413` rather than failing halfway.
- **Google Cloud Storage** is compiled in and untested. No emulator implements the API the library uses for writes, so nothing here can prove it works.

That split is the whole rule: **if it is in the panel, changing it works.** The panel never says "saved — now restart", because a setting that would need one is shown read-only with the reason instead.

Two worked examples sit in the repository root — `tessera.personal.env.example` for an instance only you can reach, and `tessera.public.env.example` for one strangers upload to. Each lists the environment half and then the panel settings worth changing for that deployment, because those two want opposite things. `contrib/tessera.service` is a systemd unit for running it outside a container.

No panel — an instance with `features.accounts` off has no session to sign in with — leaves the command line:

```bash
tessera-server settings list
tessera-server settings set share.default_expiry_hours 720
tessera-server settings reset-all        # the way back from a bad change
```

`apps/server/README.md` documents the rest, including how to strip the build down for a constrained box.

## Working on it

```bash
pnpm dev              # every frontend, in parallel
pnpm server:dev       # the backend
pnpm server:check     # lint, test, docs, audit — what CI runs
pnpm check            # lint and format the TypeScript side
```

Rust is a plain cargo workspace and is deliberately not part of the Turborepo pipeline.
