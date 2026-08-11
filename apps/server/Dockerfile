# syntax=docker/dockerfile:1.7
#
# Build context is the repository root, because the Rust workspace lives there:
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
#   pnpm server:docker

# ---------------------------------------------------------------------------
# Stage 1 — compile
# ---------------------------------------------------------------------------
FROM rust:1.97-alpine AS builder

# musl-dev/gcc build the bundled SQLite C sources; cmake, perl and clang build
# aws-lc-sys, which the S3/GCS/Azure backends pull in for signing.
RUN apk add --no-cache musl-dev gcc make cmake perl clang-dev

WORKDIR /build

# Dependency layer: workspace manifests only, compiled against stub crates so
# Docker can cache the dependency graph. Editing application source no longer
# triggers a full rebuild.
#
# The stubs carry `//!` headers because the workspace warns on `missing_docs`,
# which applies to every crate root including these throwaways.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY crates/tessera-core/Cargo.toml crates/tessera-core/
COPY apps/server/Cargo.toml apps/server/
RUN mkdir -p crates/tessera-core/src apps/server/src/bin apps/server/benches \
  && echo '//! stub' > crates/tessera-core/src/lib.rs \
  && echo '//! stub' > apps/server/src/lib.rs \
  && printf '//! stub\nfn main() {}\n' > apps/server/src/main.rs \
  && printf '//! stub\nfn main() {}\n' > apps/server/src/bin/openapi.rs \
  && printf '//! stub\nfn main() {}\n' > apps/server/benches/api.rs \
  && printf '//! stub\nfn main() {}\n' > apps/server/build.rs \
  && cargo build --release --locked --bins \
  && rm -rf crates/tessera-core/src apps/server/src apps/server/build.rs

# Real sources. `apps/web` always exists; whether it contains a `dist/` is what
# decides between the real bundle and the placeholder.
COPY crates crates
COPY apps/server apps/server
COPY apps/web apps/web

# `.git` is excluded from the build context, so build.rs cannot resolve the
# commit on its own. Pass it in to keep `/health` honest:
#   docker build --build-arg TESSERA_BUILD_REV=$(git describe --always --dirty) ...
ARG TESSERA_BUILD_REV
ENV TESSERA_BUILD_REV=${TESSERA_BUILD_REV}

# Cargo decides freshness from mtimes, and `COPY` preserves the context's mtimes
# — which can be older than the stub build above, leaving cargo convinced the
# stub is still current (build.rs in particular would never rerun, and the SPA
# would never be embedded). Dropping our own fingerprints forces a real rebuild
# of the workspace crates while leaving every dependency cached.
#
# `--locked` keeps the image reproducible: a stale Cargo.lock fails the build
# rather than silently resolving newer versions. The release profile already
# strips symbols, so there is no separate strip step.
RUN rm -rf target/release/.fingerprint/tessera-* \
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
  TESSERA_STORAGE__URL=/data/blobs \
  TESSERA_TELEMETRY__FORMAT=json \
  TESSERA_TELEMETRY__ANSI=false

EXPOSE 8787
VOLUME ["/data"]

# There is no shell and no curl in the image, so the binary probes itself.
HEALTHCHECK --interval=30s --timeout=3s --start-period=10s --retries=3 \
  CMD ["/usr/local/bin/tessera-server", "--health-check"]

ENTRYPOINT ["/usr/local/bin/tessera-server"]
