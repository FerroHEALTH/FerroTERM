#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Builds the console's site bundle (app/ferroterm-console) with
# cargo-leptos, with the workspace lockfile frozen, and checks the bundle is
# whole (no specification governs this: our own design).
#
# cargo-leptos resolves the workspace through its own `cargo metadata` call,
# which takes no --locked, and cargo has no environment variable for it. So
# the lockfile is checked with `cargo metadata --locked` first, which fails
# loud when Cargo.lock does not satisfy every manifest and leaves the second
# resolution nothing to change, and --locked is passed to the compile itself.
#
# The bundle is target/site: pkg/ferroterm-console.wasm and its JavaScript
# glue, the chunks of the lazy islands with their loader and manifest, and the
# favicon and the brand marks from public/. The server binary is built
# apart from it, with cargo, because the bundle is the same for every
# architecture.
#
# Usage: scripts/release/console-site.sh [--release]
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
console="$root/app/ferroterm-console"

case "${1:-}" in
  "" | --release) ;;
  *)
    echo "usage: scripts/release/console-site.sh [--release]" >&2
    exit 2
    ;;
esac

if ! cargo metadata --locked --format-version 1 \
  --manifest-path "$console/Cargo.toml" > /dev/null; then
  echo "console-site: Cargo.lock does not satisfy the workspace manifests;" >&2
  echo "console-site: re-resolve it deliberately and commit the change." >&2
  exit 1
fi

# The bundle names no directory of the build host: the panic locations rustc
# embeds name the registry, the toolchain and the checkout, and each is
# remapped to a fixed prefix for the WebAssembly build alone (the rustc book,
# --remap-path-prefix). Cargo splits the variable on spaces, so a directory
# with one cannot be remapped and is refused.
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
sysroot="$(rustc --print sysroot)"
for dir in "$cargo_home" "$sysroot" "$root"; do
  case "$dir" in
    *" "*)
      echo "console-site: $dir holds a space; its path cannot be remapped" >&2
      exit 1
      ;;
  esac
done
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS="--remap-path-prefix=$cargo_home=/cargo --remap-path-prefix=$sysroot=/rustc-sysroot --remap-path-prefix=$root=/ferroterm"

# cargo-leptos runs the wasm-opt it finds on PATH and downloads one unchecked
# when it finds none, so the build refuses to start until PATH holds it at its
# pinned, verified release (scripts/release/console-tools.sh installs it; no
# specification governs this: our own design).
bash "$root/scripts/release/console-tools.sh" --check

# cargo-leptos reads its configuration from the crate's own manifest
# directory, so the build runs from there.
# The chunks of an earlier build would outlive a split that no longer names
# them, so the bundle's pkg/ starts empty. --split cuts FerroUI's lazy
# islands (`#[island(lazy)]`, the tree among them) out of the entry into
# chunks the loader __wasm_split.*.js fetches by its manifest.
rm -rf -- "$root/target/site/pkg"
cd "$console"
cargo leptos build --frontend-only --split --lib-cargo-args=--locked "$@"

missing=0
for file in pkg/ferroterm-console.wasm pkg/ferroterm-console.js \
  pkg/__wasm_split_manifest.json favicon.ico; do
  if [[ ! -s "$root/target/site/$file" ]]; then
    echo "console-site: the bundle has no $file" >&2
    missing=1
  fi
done
[[ "$missing" -eq 0 ]] || exit 1
echo "console-site: the bundle is in target/site"
