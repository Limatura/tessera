# syntax=docker/dockerfile:1.7
#
# Build context is the repository root:
#   docker build -f apps/server/Dockerfile -t tessera-server .
#
# The result is a statically linked musl binary on a distroless base — no libc,
# no shell, no package manager. That is the "single binary" promise made concrete:
# the image is the binary plus a certificate bundle.
#
# `apps/web/dist` is embedded as the SPA when it is present in the context (the
# root .dockerignore un-ignores it for exactly this reason); otherwise build.rs
# embeds its placeholder page. For a complete image, build the frontend first:
#   pnpm --filter web build
#   docker build -f apps/server/Dockerfile -t tessera-server .

# ---------------------------------------------------------------------------
# Stage 1 — compile
# ---------------------------------------------------------------------------
FROM rust:1.97-alpine AS builder

# musl-dev and gcc build the bundled SQLite C sources.
RUN apk add --no-cache musl-dev gcc

WORKDIR /build/apps/server

# Dependency layer: manifests only, compiled against a stub crate so Docker can
# cache the ~400-crate dependency graph. Editing application source no longer
# triggers a full rebuild.
COPY apps/server/Cargo.toml apps/server/Cargo.lock apps/server/rust-toolchain.toml ./
RUN mkdir -p src/bin benches \
  && : > src/lib.rs \
  && echo 'fn main() {}' > src/main.rs \
  && echo 'fn main() {}' > src/bin/openapi.rs \
  && echo 'fn main() {}' > benches/api.rs \
  && echo 'fn main() {}' > build.rs \
  && cargo build --release --locked --bins \
  && rm -rf src build.rs

# Real sources. `apps/web` always exists; whether it contains a `dist/` is what
# decides between the real bundle and the placeholder.
COPY apps/server ./
COPY apps/web /build/apps/web

# Cargo decides freshness from mtimes, and `COPY` preserves the context's mtimes
# — which can be older than the stub build above, leaving cargo convinced the
# stub is still current (build.rs in particular would never rerun, and the SPA
# would never be embedded). Dropping our own fingerprint forces a real rebuild of
# this crate while leaving every dependency cached.
#
# `--locked` keeps the image reproducible: a stale Cargo.lock fails the build
# rather than silently resolving newer versions. The release profile already
# strips symbols, so there is no separate strip step.
RUN rm -rf target/release/.fingerprint/tessera-server-* \
  && cargo build --release --locked --bin tessera-server \
  && cp target/release/tessera-server /tessera-server

# ---------------------------------------------------------------------------
# Stage 2 — runtime
# ---------------------------------------------------------------------------
# `static` rather than `cc`: a musl binary needs no dynamic loader. The base is
# ~2 MB and ships no shell, so a compromised process has nothing to exec.
FROM gcr.io/distroless/static-debian12:nonroot

COPY --from=builder /tessera-server /usr/local/bin/tessera-server

# `nonroot` is uid/gid 65532; /data must be a volume or bind mount it can write.
USER nonroot:nonroot
WORKDIR /data

ENV TESSERA_SERVER__HOST=0.0.0.0 \
  TESSERA_SERVER__PORT=8787 \
  TESSERA_DATABASE__URL=sqlite:///data/tessera.db \
  TESSERA_STORAGE__LOCAL_PATH=/data/blobs \
  TESSERA_TELEMETRY__FORMAT=json \
  TESSERA_TELEMETRY__ANSI=false

EXPOSE 8787
VOLUME ["/data"]

# There is no shell and no curl in the image, so the binary probes itself.
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/tessera-server", "--health-check"]

ENTRYPOINT ["/usr/local/bin/tessera-server"]
