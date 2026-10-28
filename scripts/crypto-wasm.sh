#!/usr/bin/env bash
# Builds the browser's copies of the encryption format into packages/crypto and
# of the metadata remover into packages/metadata.
#
# The output is committed, and CI runs this again and fails if it differs. That
# keeps Rust out of the JavaScript toolchain: a frontend change never needs a
# Rust compiler, and the module the browser loads is always the crate as it is.
#
# Byte-for-byte reproducible on one toolchain, which is what the check needs.
# What differs between machines is where sources sit, which panic messages
# carry, so every prefix is rewritten to a fixed name. The standard library's is
# the subtle one: with the `rust-src` component installed, rustc names the real
# files under the sysroot, and without it the virtual `/rustc/<commit>` path, so
# the real one is mapped back to the virtual one.
#
# Needs the `wasm32-unknown-unknown` target and `wasm-bindgen` at the exact
# version `crates/tessera-crypto/Cargo.toml` pins.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

pinned=$(sed -n 's/^wasm-bindgen = { version = "=\([0-9.]*\)".*/\1/p' crates/tessera-crypto/Cargo.toml)
installed=$(wasm-bindgen --version | awk '{print $2}')
if [ "$pinned" != "$installed" ]; then
  echo "wasm-bindgen $installed is installed; the crate pins $pinned" >&2
  echo "  cargo install wasm-bindgen-cli --version $pinned --locked" >&2
  exit 1
fi

sysroot=$(rustc --print sysroot)
commit=$(rustc -vV | sed -n 's/^commit-hash: //p')

RUSTFLAGS="--remap-path-prefix=$root=/tessera \
--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo \
--remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$commit" \
  cargo rustc --locked -p tessera-crypto --lib --crate-type cdylib --features wasm \
  --target wasm32-unknown-unknown --profile wasm

RUSTFLAGS="--remap-path-prefix=$root=/tessera \
--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}=/cargo \
--remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$commit" \
  cargo rustc --locked -p tessera-metadata --lib --crate-type cdylib --features wasm \
  --target wasm32-unknown-unknown --profile wasm

# CI builds into a shared directory outside the checkout, not `target/`.
#
# `--remove-producers-section` because that section names how `wasm-bindgen`
# itself was built: the release binary CI installs adds its commit, one built
# with `cargo install` does not, and the output differed by exactly that.
wasm-bindgen --target web --remove-producers-section \
  --out-dir packages/crypto/src/wasm --out-name tessera_crypto \
  "${CARGO_TARGET_DIR:-$root/target}/wasm32-unknown-unknown/wasm/tessera_crypto.wasm"
wasm-bindgen --target web --remove-producers-section \
  --out-dir packages/metadata/src/wasm --out-name tessera_metadata \
  "${CARGO_TARGET_DIR:-$root/target}/wasm32-unknown-unknown/wasm/tessera_metadata.wasm"
