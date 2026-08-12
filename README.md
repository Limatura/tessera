# tessera

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
apps/web         the app people use, React + TanStack Router
apps/admin       the operator's console
apps/tui         a terminal client
apps/extension   a browser extension
apps/fumadocs    the documentation site
packages/ui      shadcn/ui primitives shared by web and admin
packages/i18n    locale plumbing shared by web and admin
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

Defaults, then `tessera.yaml`, then the environment. Environment variables are prefixed `TESSERA_`, with `__` separating nesting levels:

```bash
TESSERA_SERVER__PORT=9000
TESSERA_DATABASE__URL=sqlite://data/tessera.db      # or postgres:// / mysql://
TESSERA_STORAGE__URL=data/blobs                     # or s3:// / gs:// / az:// / https://
```

Every backend is compiled in by default, so pointing at S3 or Postgres needs no rebuild.

On first run the server writes a `tessera.yaml` next to itself with every setting commented out, showing the values it is actually using. Two worked examples sit in the repository root — `tessera.personal.yaml` for an instance only you can reach, and `tessera.public.yaml` for one strangers upload to. They disagree on almost every line, because those two deployments want opposite things.

`apps/server/README.md` documents the rest, including how to strip the build down for a constrained box.

## Working on it

```bash
pnpm dev              # every frontend, in parallel
pnpm server:dev       # the backend
pnpm server:check     # lint, test, docs, audit — what CI runs
pnpm check            # lint and format the TypeScript side
```

Rust is a plain cargo workspace and is deliberately not part of the Turborepo pipeline.
