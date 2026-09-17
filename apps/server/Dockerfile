# syntax=docker/dockerfile:1.7
#
# Build context is the repository root, because the Rust workspace lives there:
#   docker build -f apps/server/Dockerfile -t tessera-server .
#
# The result is a statically linked musl binary on a distroless base — no libc,
# no shell, no package manager. That is the "single binary" promise made concrete:
# the image is the binary plus a certificate bundle.
#
# The SPA is built here too, in its own stage, so that `docker build` alone
# produces a complete image. It used to depend on `apps/web/dist` already being
# in the context, which meant the image was only as good as whatever the caller
# happened to have built last — and CI, which never built it, was validating an
# image that carried build.rs's placeholder page.

# ---------------------------------------------------------------------------
# Stage 1 — the SPA
# ---------------------------------------------------------------------------
# Node only exists in this stage. Nothing it installs reaches the runtime image;
# what crosses the boundary is `dist`, which is HTML, CSS and JavaScript.
#
# `$BUILDPLATFORM`, because what comes out of here is the same bytes whatever
# the image is for. Without it a `--platform linux/arm64` build pulls an arm64
# Node and tries to run it on the amd64 builder: `exec /bin/sh: exec format
# error`, before a single line of the SPA is compiled.
FROM --platform=$BUILDPLATFORM node:24-alpine AS web

RUN corepack enable

WORKDIR /build

# Manifests first, so the install layer survives a source-only change. Every
# workspace package `web` depends on has to be here, or pnpm refuses to resolve
# `workspace:*`.
COPY package.json pnpm-lock.yaml pnpm-workspace.yaml ./
COPY apps/web/package.json apps/web/
COPY packages/api/package.json packages/api/
COPY packages/auth/package.json packages/auth/
COPY packages/ui/package.json packages/ui/
COPY packages/i18n/package.json packages/i18n/
COPY packages/env/package.json packages/env/
COPY packages/config/package.json packages/config/

# `--filter web...` pulls in web and everything it depends on, and nothing else
# — no admin, no docs site, no extension. `--frozen-lockfile` makes an undeclared
# dependency fail the build instead of quietly resolving.
RUN --mount=type=cache,target=/root/.local/share/pnpm/store \
  pnpm install --frozen-lockfile --filter web...

COPY packages packages
COPY apps/web apps/web

# Vite plugins generate paraglide's runtime and TanStack's route tree during
# this, which is why there is no separate codegen step.
RUN pnpm --filter web build

# ---------------------------------------------------------------------------
# Stage 2 — compile
# ---------------------------------------------------------------------------
# Cross-compiled, not emulated. The audience runs this on a NAS or a Pi, so
# arm64 is not an afterthought — but `aws-lc-sys` and the bundled SQLite are C,
# and building them under QEMU turns a four-minute job into a long one. These
# images carry a real musl cross toolchain and run natively on the amd64
# builder, so both architectures cost about the same.
#
# One stage per target, selected by `TARGETARCH` below. `CARGO_BUILD_TARGET` is
# what makes the rest of the stage architecture-agnostic: every `cargo build`
# after this point honours it without a `--target` flag, and `$CARGO_BUILD_TARGET`
# names the output directory.
FROM --platform=$BUILDPLATFORM messense/rust-musl-cross:x86_64-musl AS toolchain-amd64
ENV CARGO_BUILD_TARGET=x86_64-unknown-linux-musl

FROM --platform=$BUILDPLATFORM messense/rust-musl-cross:aarch64-musl AS toolchain-arm64
ENV CARGO_BUILD_TARGET=aarch64-unknown-linux-musl

ARG TARGETARCH
FROM toolchain-${TARGETARCH} AS builder

WORKDIR /build

# `rust-toolchain.toml` pins the compiler, so the target rustup preinstalled for
# the image's own default toolchain does not apply to the one that actually
# runs. Installing it here rather than discovering it at the first build:
# without this every crate fails with "the target may not be installed".
COPY rust-toolchain.toml ./
RUN rustup target add "$CARGO_BUILD_TARGET"

# Dependency layer: workspace manifests only, compiled against stub crates so
# Docker can cache the dependency graph. Editing application source no longer
# triggers a full rebuild.
#
# The stubs carry `//!` headers because the workspace warns on `missing_docs`,
# which applies to every crate root including these throwaways.
COPY Cargo.toml Cargo.lock ./
#
# Every workspace member needs its manifest here, the ones the server does not
# depend on included: `members` is a glob, and a member missing from it is a
# lock file that no longer matches, which `--locked` refuses.
COPY crates/tessera-core/Cargo.toml crates/tessera-core/
COPY crates/tessera-crypto/Cargo.toml crates/tessera-crypto/
COPY apps/server/Cargo.toml apps/server/
RUN mkdir -p crates/tessera-core/src crates/tessera-crypto/src apps/server/src/bin apps/server/benches \
  && echo '//! stub' > crates/tessera-core/src/lib.rs \
  && echo '//! stub' > crates/tessera-crypto/src/lib.rs \
  && echo '//! stub' > apps/server/src/lib.rs \
  && printf '//! stub\nfn main() {}\n' > apps/server/src/main.rs \
  && printf '//! stub\nfn main() {}\n' > apps/server/src/bin/openapi.rs \
  && printf '//! stub\nfn main() {}\n' > apps/server/benches/api.rs \
  && printf '//! stub\nfn main() {}\n' > apps/server/build.rs \
  && cargo build --release --locked --bins \
  && rm -rf crates/tessera-core/src crates/tessera-crypto/src apps/server/src apps/server/build.rs

# Real sources. `apps/web` is copied for its `index.html`, which build.rs reads;
# the bundle itself arrives from the stage above rather than from the context,
# so a stale local `dist` cannot end up in the image.
COPY crates crates
COPY apps/server apps/server
COPY apps/web apps/web
COPY --from=web /build/apps/web/dist apps/web/dist

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
# Both fingerprint directories, and that is not belt-and-braces. Cross-compiling
# splits the package in two: the build script is compiled and run for the
# *host*, so its fingerprint lives in `target/release`, while the crate itself
# lives under the target triple. Clearing only the triple leaves cargo certain
# build.rs is current, it never reruns, `assets/dist` is never created, and
# `RustEmbed` fails on a folder that does not exist — which is exactly how this
# failed the first time.
#
# `--locked` keeps the image reproducible: a stale Cargo.lock fails the build
# rather than silently resolving newer versions. The release profile already
# strips symbols, so there is no separate strip step.
RUN rm -rf "target/release/.fingerprint/tessera-"* \
  "target/$CARGO_BUILD_TARGET/release/.fingerprint/tessera-"* \
  && cargo build --release --locked --bin tessera-server \
  && cp "target/$CARGO_BUILD_TARGET/release/tessera-server" /tessera-server

# ---------------------------------------------------------------------------
# Stage 3 — runtime
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
