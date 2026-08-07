# tessera-server

The Rust backend. All business logic lives here; `apps/web` is a pure SPA whose
build output is compiled into this binary.

A release build is **one executable** — API, static assets and Swagger UI in a
single process, with no nginx and no Node runtime in front of it.

## Quick start

```bash
cargo run                    # http://127.0.0.1:8787
curl localhost:8787/health
open http://localhost:8787/docs
```

From the repository root the same things are available as `pnpm server:dev`,
`pnpm server:test`, `pnpm server:lint`, `pnpm server:audit`, `pnpm server:openapi`.

## Layout

Modules are organised by capability, not by MVC layer:

```
src/
├── main.rs        start-up, shutdown, and the --health-check self-probe
├── lib.rs         module tree and lint policy pointer
├── config.rs      figment loading + fail-fast validation
├── error.rs       the crate error type and its single IntoResponse
├── telemetry.rs   tracing subscriber (pretty | json)
├── state.rs       AppState and the service lifecycle flag
├── db/            sqlx pool + migration runner (SQLite, optional Postgres)
├── storage/       object_store abstraction (local FS, optional S3)
├── api/           route assembly, middleware stack, OpenAPI document
├── assets.rs      rust-embed SPA with client-side-routing fallback
└── bin/openapi.rs writes ../../openapi.json
```

## Configuration

Precedence, lowest to highest: built-in defaults → `tessera.toml` → environment.
Environment variables are prefixed `TESSERA_`, with `__` separating nesting
levels:

```bash
TESSERA_SERVER__PORT=9000
TESSERA_SERVER__HOST=0.0.0.0
TESSERA_DATABASE__URL=sqlite://data/tessera.db
TESSERA_STORAGE__LOCAL_PATH=data/blobs
TESSERA_TELEMETRY__FORMAT=json      # pretty | json
TESSERA_TELEMETRY__FILTER=info,tessera_server=debug
```

Point at a different file with `TESSERA_CONFIG_FILE=/etc/tessera.toml`. A missing
file is not an error.

Everything is validated during start-up: a bad port, an unusable log filter, an
unknown key, or a backend the binary was not compiled with all fail the process
before it opens a socket. See `src/config.rs`.

### Cargo features

| Feature    | Default | Effect                                                   |
| ---------- | ------- | -------------------------------------------------------- |
| `postgres` | off     | accept `postgres://` URLs alongside the built-in SQLite   |
| `s3`       | off     | accept `storage.backend = "s3"` for S3-compatible buckets |

Both are off by default because the homelab target runs on SQLite and a local
directory, and each optional backend costs build time and binary size. Selecting
a backend the binary lacks is a start-up error naming the missing feature.

## The OpenAPI contract

`openapi.json` at the repository root is **generated, never hand-edited**:

```bash
cargo run --bin tessera-openapi     # rewrites ../../openapi.json
```

The frontend and the Discord bot generate their clients from that file. CI
regenerates it and fails on any difference, so a handler change that is not
reflected in the contract cannot merge.

## Toolchain

| Tool            | What it is for                                | Command                                                    |
| --------------- | --------------------------------------------- | ---------------------------------------------------------- |
| `rustfmt`       | formatting, incl. import grouping             | `cargo +nightly fmt --all` (`--check` in CI)                |
| `clippy`        | lints, `all` + `pedantic` + selected `restriction` | `cargo clippy --all-targets --all-features -- -D warnings` |
| `cargo-nextest` | test runner                                   | `cargo nextest run`                                         |
| `proptest`      | property-based tests                          | part of `cargo nextest run`                                 |
| `insta`         | snapshot tests (the OpenAPI document)         | `cargo insta review` after an intended change               |
| `rstest`        | parameterised tests                           | part of `cargo nextest run`                                 |
| `criterion`     | benchmarks                                    | `cargo bench --bench api`                                   |
| `cargo-mutants` | mutation testing                              | `cargo mutants` (scheduled, not in PRs)                     |
| `cargo-deny`    | licence and advisory policy                   | `cargo deny check`                                          |
| `cargo-audit`   | vulnerability scan                            | `cargo audit`                                               |
| `cargo-machete` | unused dependency detection                   | `cargo machete`                                             |

Install what you do not have:

```bash
cargo install cargo-nextest cargo-deny cargo-audit cargo-machete cargo-mutants cargo-insta --locked
rustup toolchain install nightly --profile minimal --component rustfmt
```

Nightly is needed **only** for formatting: `rustfmt.toml` uses `group_imports`
and `imports_granularity`, which are still unstable. Everything else builds on
the pinned stable toolchain in `rust-toolchain.toml`.

## Docker

Built from the repository root so the SPA bundle is in scope:

```bash
pnpm --filter web build                                   # optional, but see below
docker build -f apps/server/Dockerfile -t tessera-server .
docker run -p 8787:8787 -v tessera:/data tessera-server
```

Without `apps/web/dist`, `build.rs` embeds a placeholder page instead of the
real SPA and the API still works.

The image is a statically linked musl binary on `distroless/static` — no shell,
no libc, no package manager. CI enforces a 50 MB budget; the current image is
well under 10 MB. Because there is no shell, `HEALTHCHECK` re-executes the binary
with `--health-check`, which probes `/health` over a raw socket.

## Testing

```bash
cargo nextest run                    # everything
cargo nextest run --profile ci       # retries + junit output, as CI runs it
cargo insta review                   # after an intentional contract change
```

Integration tests in `tests/` drive the assembled router through `tower`'s
`oneshot`, backed by an in-memory SQLite database and a temporary blob
directory — the same stack `main.rs` serves, with no service wrapper in between.

## Migrations

`migrations/sqlite/` and `migrations/postgres/` are kept separate because sqlx
checksums migrations per dialect and the two diverge as soon as real tables
appear. They run automatically at start-up unless
`TESSERA_DATABASE__RUN_MIGRATIONS=false`.
