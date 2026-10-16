#!/usr/bin/env bash
# Copies apps/server/package.json's version into the Cargo workspace version.
# Run after `changeset version`; every crate inherits it via `version.workspace`.
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(node -p "require('./apps/server/package.json').version")

sed -i '/^\[workspace\.package\]/,/^\[/ s/^version = ".*"/version = "'"$version"'"/' Cargo.toml

# Refreshes the workspace members' entries in Cargo.lock without touching dependencies.
cargo update --workspace

# The contract carries the crate version as `info.version`, and CI fails when
# the committed file drifts from the generated one.
cargo run --locked --quiet --bin tessera-openapi

echo "Cargo workspace version -> $version"
