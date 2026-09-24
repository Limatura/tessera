# tessera-server

The Rust backend. All business logic lives here; `apps/web` is a pure SPA whose
build output is compiled into this binary.

A release build is **one executable** — API, static assets and Swagger UI in a
single process, with no nginx and no Node runtime in front of it.

## Quick start

```bash
cargo run --bin tessera-server     # http://127.0.0.1:8787
curl localhost:8787/health
open http://localhost:8787/docs
```

From the repository root the same things are available as `pnpm server:dev`,
`server:test`, `server:lint`, `server:doc`, `server:audit`, `server:openapi`, and
`pnpm server:check` to run the lot.

## Layout

The Rust side is a cargo workspace rooted at the repository root, alongside the
pnpm workspace. `apps/*` holds deployables in both worlds; `packages/*` holds
JavaScript libraries and `crates/*` holds Rust ones.

```
Cargo.toml              workspace root: members, shared deps, lint policy
crates/tessera-core/    domain rules — pure, no IO, no framework
apps/server/            this crate: HTTP, database, storage, embedded SPA
```

Two crates, not four. The split exists for one reason: it is the only way in Rust
to make "business logic does not belong in a handler" a **compile error** rather
than a review comment. `tessera-core` does not depend on `axum`, `sea-orm`,
`object_store` or `tokio`, so code that reaches for them does not build — and CI
walks the resolved tree to catch one arriving transitively.

Inside `apps/server`, modules are organised by capability rather than by layer:

```
src/
├── main.rs        start-up, shutdown, and the --health-check self-probe
├── lib.rs         module tree; re-exports tessera-core as `domain`
├── config.rs      figment loading + fail-fast validation
├── error.rs       the crate error type and its single IntoResponse
├── telemetry.rs   tracing subscriber (pretty | json)
├── state.rs       AppState and the service lifecycle flag
├── db/            SeaORM connection, migrations (SQLite | Postgres | MySQL)
├── storage/       object_store abstraction (local, S3, GCS, Azure, WebDAV)
├── api/           route assembly, middleware stack, OpenAPI document
├── assets.rs      rust-embed SPA with client-side-routing fallback
└── bin/openapi.rs writes ../../openapi.json
```

## Configuration

Precedence, lowest to highest: built-in defaults → environment → the `settings`
table the admin panel writes.

Only seventeen settings can come from the environment, and they are the ones a
running process cannot pick up: the listening address (the socket is bound), the
database (the pool is built, and reading the settings table needs it), the log
format (the subscriber is installed first), the middleware layers baked into the
router at build time, and the values that hold a credential. Everything else is
in the panel, or in `tessera-server settings set`.

Environment variables are prefixed `TESSERA_`, with `__` separating nesting
levels:

```bash
TESSERA_SERVER__PORT=9000
TESSERA_SERVER__HOST=0.0.0.0
TESSERA_DATABASE__URL=sqlite://data/tessera.db     # or postgres:// / mysql://
TESSERA_STORAGE__URL=data/blobs                    # or s3:// / gs:// / az:// / https://
TESSERA_STORAGE__OPTIONS__ENDPOINT=http://minio:9000
TESSERA_TELEMETRY__FORMAT=json                     # pretty | json
TESSERA_TELEMETRY__FILTER=info,tessera_server=debug
```

`tessera-server settings list` prints every setting, its value, and which layer
supplied it. The ones the panel cannot change are marked read-only with the
reason.

Settings you want on every run of a development server go in `.dev/local.env`,
one `NAME=value` per line. `pnpm server:dev` and `pnpm server:dev:fakes` both
load it when it exists, after `fakes.env`, so it wins over the fakes. It is not
in version control, and `pnpm dev:fakes` never rewrites it. This is the way to
set the boot-fixed ones, which the panel refuses by design:

```bash
# .dev/local.env
TESSERA_FEATURES__DOCS=true     # mounts the browsable API docs at /docs
```

Everything is validated during start-up: a bad port, an unusable log filter, an
unknown key, or a backend the binary was not compiled with all fail the process
before it opens a socket.

### Storage

`storage.url` selects the backend. A bare path is a local directory, created if
missing; anything with a scheme is handed to `object_store`:

| Value                      | Backend             |
| -------------------------- | ------------------- |
| `data/blobs`               | local filesystem    |
| `s3://bucket/prefix`       | S3 / MinIO / Garage |
| `gs://bucket`              | Google Cloud        |
| `az://container`           | Azure               |
| `https://host/dav`         | WebDAV / HTTP       |
|  `memory:///`              | in-process (tests)  |

A URL may carry a path prefix — `s3://bucket/tessera` keeps objects under
`tessera/` rather than at the bucket root, so a shared bucket stays tidy.

`storage.options` is passed through verbatim, so any option the backend
understands is reachable without waiting for a release. The standard `AWS_*`,
`GOOGLE_*` and `AZURE_*` environment variables are merged in automatically —
but only the ones `object_store` actually recognises. That filter is not
cosmetic: `object_store` rejects an unknown key outright, so forwarding
everything `AWS_*` would mean any machine with an ordinary AWS CLI setup
(`AWS_PROFILE`, `AWS_PAGER`) could not start the server. Anything in `options`
wins over the environment.

### Reserved environment variables

`TESSERA_BUILD_REV` is exported into every `cargo run` by the build script, and
`TESSERA_CONFIG_FILE` is what an instance from before the panel still has set —
it no longer does anything, and start-up says so. `Config` is
`deny_unknown_fields`, so both are ignored explicitly (see
`NON_CONFIG_ENV_KEYS`); anything else prefixed `TESSERA_` that is not a real key
still stops the process, which is how a typo gets caught.

### Cargo features

Every backend is on by default: `docker pull` should give you all of them, and
nobody should rebuild to point at S3. The flags exist so a constrained box can
strip back down.

| Feature                                                    | Default |
| ---------------------------------------------------------- | ------- |
| `sqlite`, `postgres`, `mysql`                               | on      |
| `storage-fs`, `storage-s3`, `storage-gcs`, `storage-azure`, `storage-http` | on |

```bash
# SQLite + local disk only — the smallest useful build
cargo build --release --no-default-features --features sqlite,storage-fs
```

Selecting a backend the binary lacks is a start-up error naming the missing
feature. CI builds the feature powerset with `cargo hack`, because testing only
the empty and full sets leaves the middle combinations unbuilt — which is exactly
where a missing `#[cfg]` hides.

## Data layer

SeaORM, over sqlx. One `DatabaseConnection` covers all three dialects, so a query
is written once instead of once per backend.

What that gives up is `sqlx::query!`'s compile-time schema check — but that macro
only ever verified *one* dialect, so committing to three removed it regardless of
ORM. It is replaced by CI running the real migrations and queries against real
Postgres and MySQL servers: narrower feedback, wider coverage.

**Migrations are hand-written SQL, one file per dialect**, in
`src/db/migration/sql/`. SeaORM's schema builder would collapse them into one
definition, and that was the obvious choice — but the table definition is the
part people need to read, and `CREATE TABLE shares (...)` beats a chain of
`ColumnDef::new(...).text().not_null()` for a reviewer or an operator. The Rust
migration file only dispatches on the backend. The risk this buys — one dialect
updated, the others forgotten — is guarded by a unit test comparing statement
counts and by the CI job that applies all three for real.

SQLite gets its pool built by hand and handed to SeaORM, because `ConnectOptions`
cannot express pragmas and sqlx's URL parser rejects unknown query parameters.
That is what keeps WAL, `busy_timeout`, `foreign_keys` and incremental
`auto_vacuum` — see `db::connect_sqlite`.

## Footprint

"Functional, but not bloated" is a product decision, so it is a number the build
enforces rather than an argument to have per pull request:

| Metric                              | Current | Budget |
| ----------------------------------- | ------- | ------ |
| binary (musl static, stripped)      | 23.7 MB | 28 MB  |
| image rootfs (`docker export`)      | 26.8 MB | 34 MB  |
| startup RSS                         | 8.6 MB  | —      |

Three numbers get quoted for container images and they differ a lot: this one
pulls as ~10 MB (compressed), occupies ~39 MB by `docker image ls` (layer store
overhead), and contains 26.8 MB of actual files. CI gates the last one, because
`docker export` flattens the filesystem and therefore means the same thing
whichever image store the daemon uses.

For reference, the same server before SeaORM and the extra backends was a 17.2 MB
binary with 5.2 MB RSS. Three SQL dialects and four object stores cost ~6.5 MB and
~3.4 MB respectively.

## The OpenAPI contract

`openapi.json` at the repository root is **generated, never hand-edited**:

```bash
pnpm server:openapi        # cargo run --bin tessera-openapi
```

The frontend and the Discord bot generate their clients from that file. CI
regenerates it and fails on any difference, so a handler change that is not
reflected in the contract cannot merge.

## Toolchain

| Tool            | What it is for                                    | Command                                                             |
| --------------- | ------------------------------------------------- | ------------------------------------------------------------------- |
| `rustfmt`       | formatting, incl. import grouping                 | `cargo +nightly fmt --all` (`--check` in CI)                         |
| `clippy`        | lints — see below                                 | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| `cargo-nextest` | test runner                                       | `cargo nextest run --workspace`                                      |
| `proptest`      | property-based tests                              | part of the test run                                                 |
| `insta`         | snapshot tests (the OpenAPI document)             | `cargo insta review` after an intended change                        |
| `rstest`        | parameterised tests                               | part of the test run                                                 |
| `criterion`     | benchmarks                                        | `cargo bench --bench api`                                            |
| `cargo-hack`    | feature-powerset build check                      | `cargo hack --feature-powerset --depth 2 --no-dev-deps check`        |
| `cargo-mutants` | mutation testing                                  | `cargo mutants` (scheduled, not in PRs)                              |
| `cargo-deny`    | advisories, licences, banned crates, sources      | `cargo deny check`                                                   |
| `cargo-machete` | unused dependency detection                       | `cargo machete`                                                      |
| `rustdoc`       | broken intra-doc links, malformed examples        | `cargo doc --workspace --no-deps --all-features`                     |

```bash
cargo install cargo-nextest cargo-deny cargo-machete cargo-mutants cargo-insta cargo-hack --locked
rustup toolchain install nightly --profile minimal --component rustfmt
```

Nightly is needed **only** for formatting: `rustfmt.toml` uses `group_imports`
and `imports_granularity`, which are still unstable. Everything else builds on
the pinned stable toolchain in `rust-toolchain.toml`.

There is no `cargo-audit`. It scans `Cargo.lock`, which lists crates the resolver
merely considered; `cargo-deny` resolves the real graph with features applied.
The difference is not academic — see the comment in `deny.toml`.

### Lint policy

Levels live in the workspace `Cargo.toml` under `[workspace.lints]`, in two tiers:

- **`deny`** — wrong at any moment, including mid-experiment. Blocks `cargo run`.
  Held to concurrency mistakes and silent truncation: `await_holding_lock`,
  `arc_with_non_send_sync`, `rc_mutex`, `as_conversions`, `unsafe_code`.
- **`warn`** — should not merge, but must not stop you iterating. Everything else,
  including all of `pedantic`. CI runs `-D warnings`, which makes them blocking
  there.

That split is the point: a contributor with a dozen pedantic warnings can still
run the server and the tests, and finds out at pull-request time — usually
fixable with `cargo clippy --fix`.

`clippy.toml` only tunes behaviour: thresholds, the test-code exemptions, the
disallowed-method list. Every suppression must carry a `reason`, and
`clippy::allow_attributes` pushes you towards `#[expect]`, which becomes a warning
once it stops being needed.

## Testing

```bash
cargo nextest run --workspace        # everything that needs no external service
cargo insta review                   # after an intentional contract change
```

Integration tests in `tests/` drive the assembled router through `tower`'s
`oneshot`, backed by an in-memory SQLite database and a temporary blob directory —
the same stack `main.rs` serves, with no service wrapper in between.

### Postgres and MySQL

The backend suite is `#[ignore]`d, because it needs a server a plain test run has
no reason to provide. Ignoring rather than skipping is deliberate: a test that
returns early when an environment variable is missing reports "passed" and
teaches you to trust coverage that never ran.

```bash
docker run -d --name tessera-pg -p 5432:5432 \
  -e POSTGRES_USER=tessera -e POSTGRES_PASSWORD=tessera \
  -e POSTGRES_DB=tessera_test postgres:17-alpine

TESSERA_TEST_DATABASE_URL=postgres://tessera:tessera@localhost:5432/tessera_test \
  pnpm server:test:backends
```

One suite covers every dialect; the URL decides which. CI runs it as a matrix
against both on every pull request.

## Build identification

`/health` reports `build_rev` alongside `version`. Between releases the version
number is identical across dozens of builds, so this is what makes a bug report
actionable. It comes from `git describe` at compile time; the Docker build has no
`.git`, so pass it in — `pnpm server:docker` does this for you:

```bash
docker build -f apps/server/Dockerfile \
  --build-arg TESSERA_BUILD_REV=$(git describe --always --dirty --abbrev=12) \
  -t tessera-server .
```

## Docker

Built from the repository root, because the cargo workspace lives there:

```bash
pnpm --filter web build     # optional; without it a placeholder page is embedded
pnpm server:docker
docker run -p 8787:8787 -v tessera:/data tessera-server
```

The image is a statically linked musl binary on `distroless/static` — no shell,
no libc, no package manager. Because there is no shell, `HEALTHCHECK`
re-executes the binary with `--health-check`, which probes `/health` over a raw
socket rather than pulling in an HTTP client.
