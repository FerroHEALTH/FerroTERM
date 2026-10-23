#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Installs the two build tools cargo-leptos runs for the console's
# site bundle, the Tailwind CSS standalone CLI and Binaryen's wasm-opt, each
# at its pinned release and checked against its pinned SHA-256 before it is
# kept, so cargo-leptos finds both on PATH and never downloads either itself
# (no specification governs this: our own design).
#
# cargo-leptos 0.3.11 runs a `tailwindcss` or `wasm-opt` it finds on PATH, and
# otherwise downloads the release archive and runs it unchecked. The versions
# and digests below are the rows of docs/VERSIONS.md, which
# scripts/checks/versions.sh holds them to; each digest equals the one the
# upstream release publishes beside its asset.
#
# Usage:
#   scripts/release/console-tools.sh DIR   install both into DIR/bin; put
#                                         DIR/bin on PATH before the build
#   scripts/release/console-tools.sh --check
#                                         fail unless PATH holds both at
#                                         their pinned versions and digests
set -Eeuo pipefail

tailwind_version="4.3.3"
tailwind_linux_x64_sha256="dc61b3ac6b8c9ca874c0cc4c57b2409791a64c5540404ca5f5367360babc313a"
tailwind_macos_arm64_sha256="cdf646702987a743464dff4d9c60fd4480d1c1e73dd819a9a67f1078815dce9d"
binaryen_version="version_133"
binaryen_x86_64_linux_sha256="2dc9c7813f5375db93d96ead4b78222fcc3e2677bbb832297af4797782a37489"
binaryen_arm64_macos_sha256="ad66da82ac13f163e424b1643f16c6dfcccc98b5966296b43e52d3cab04f84a8"
# The files inside each Binaryen archive that --check digests: wasm-opt, and
# on macOS the libbinaryen it loads, which holds the optimizer itself.
wasm_opt_x86_64_linux_sha256="8f25e9fd5db0fc5f210003aaa432922feb2e52d309e430def2f929e34da9466b"
wasm_opt_arm64_macos_sha256="81041e09f332df94db1c2009a64d8f3b85f0431a2c920ccd014d3ceebb343402"
libbinaryen_arm64_macos_sha256="61055e190d84d5db6d1dec63456e0c24dad324ecfd4d2e23740dca217ed89e5a"

die() {
  echo "console-tools: $*" >&2
  exit 1
}

# Prints the SHA-256 of FILE, with the tool the platform has.
digest_of() {
  if command -v sha256sum > /dev/null; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# Checks FILE against the SHA-256 DIGEST.
verify() {
  local file="$1" digest="$2" found
  found="$(digest_of "$file")"
  [ "$found" = "$digest" ] || die "$file has SHA-256 $found, the pin is $digest"
}

# Prints FILE with every symbolic link on its way resolved, so the digest is
# taken of the file a link on PATH runs.
resolve() {
  local file="$1" link
  while [ -L "$file" ]; do
    link="$(readlink "$file")"
    case "$link" in
      /*) file="$link" ;;
      *) file="$(dirname "$file")/$link" ;;
    esac
  done
  (cd -P "$(dirname "$file")" && printf '%s/%s\n' "$(pwd -P)" "$(basename "$file")")
}

# Prints the first line a tool printed, with any ANSI escape removed: the
# Tailwind CSS 4 CLI colours its banner even when its output is a pipe.
first_line() {
  NO_COLOR=1 FORCE_COLOR=0 "$@" 2>&1 | head -n 1 | sed -E $'s/\x1b\\[[0-9;]*[A-Za-z]//g' || true
}

# Fails unless the tools on PATH are the pinned releases, by version and by
# the SHA-256 of the files that run.
check() {
  local said tailwind wasm_opt tailwind_sha wasm_opt_sha libbinaryen_sha=""
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      tailwind_sha="$tailwind_linux_x64_sha256"
      wasm_opt_sha="$wasm_opt_x86_64_linux_sha256"
      ;;
    Darwin-arm64)
      tailwind_sha="$tailwind_macos_arm64_sha256"
      wasm_opt_sha="$wasm_opt_arm64_macos_sha256"
      libbinaryen_sha="$libbinaryen_arm64_macos_sha256"
      ;;
    *) die "no pinned digest for $(uname -s) $(uname -m), so the tools on PATH cannot be checked" ;;
  esac
  tailwind="$(command -v tailwindcss)" ||
    die "no tailwindcss on PATH; run scripts/release/console-tools.sh DIR and put DIR/bin on PATH"
  wasm_opt="$(command -v wasm-opt)" ||
    die "no wasm-opt on PATH; run scripts/release/console-tools.sh DIR and put DIR/bin on PATH"
  said="$(first_line tailwindcss --help)"
  case "$said" in
    *"tailwindcss v$tailwind_version"*) ;;
    *) die "tailwindcss on PATH says '$said', not v$tailwind_version" ;;
  esac
  said="$(first_line wasm-opt --version)"
  case "$said" in
    *"($binaryen_version)"*) ;;
    *) die "wasm-opt on PATH says '$said', not $binaryen_version" ;;
  esac
  tailwind="$(resolve "$tailwind")"
  wasm_opt="$(resolve "$wasm_opt")"
  verify "$tailwind" "$tailwind_sha"
  verify "$wasm_opt" "$wasm_opt_sha"
  if [ -n "$libbinaryen_sha" ]; then
    verify "$(dirname "$wasm_opt")/../lib/libbinaryen.dylib" "$libbinaryen_sha"
  fi
}

# Downloads URL into FILE.
fetch() {
  curl --fail --silent --show-error --location --retry 3 --output "$2" "$1"
}

install() {
  local dir="$1" tailwind_asset tailwind_sha binaryen_asset binaryen_sha
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      tailwind_asset="tailwindcss-linux-x64"
      tailwind_sha="$tailwind_linux_x64_sha256"
      binaryen_asset="binaryen-$binaryen_version-x86_64-linux.tar.gz"
      binaryen_sha="$binaryen_x86_64_linux_sha256"
      ;;
    Darwin-arm64)
      tailwind_asset="tailwindcss-macos-arm64"
      tailwind_sha="$tailwind_macos_arm64_sha256"
      binaryen_asset="binaryen-$binaryen_version-arm64-macos.tar.gz"
      binaryen_sha="$binaryen_arm64_macos_sha256"
      ;;
    *) die "no pinned digest for $(uname -s) $(uname -m); install both tools by hand" ;;
  esac
  mkdir -p "$dir/bin" "$dir/download"
  fetch "https://github.com/tailwindlabs/tailwindcss/releases/download/v$tailwind_version/$tailwind_asset" \
    "$dir/download/$tailwind_asset"
  verify "$dir/download/$tailwind_asset" "$tailwind_sha"
  cp "$dir/download/$tailwind_asset" "$dir/bin/tailwindcss"
  chmod 755 "$dir/bin/tailwindcss"
  fetch "https://github.com/WebAssembly/binaryen/releases/download/$binaryen_version/$binaryen_asset" \
    "$dir/download/$binaryen_asset"
  verify "$dir/download/$binaryen_asset" "$binaryen_sha"
  tar -xzf "$dir/download/$binaryen_asset" -C "$dir"
  # The macOS wasm-opt loads libbinaryen from the lib directory beside its
  # own, so it stays in its tree and PATH reaches it through a link.
  ln -sf "$dir/binaryen-$binaryen_version/bin/wasm-opt" "$dir/bin/wasm-opt"
  PATH="$dir/bin:$PATH" check
  echo "console-tools: tailwindcss v$tailwind_version and wasm-opt $binaryen_version are in $dir/bin"
}

case "${1:-}" in
  --check) check ;;
  "" | -*)
    echo "usage: scripts/release/console-tools.sh DIR | --check" >&2
    exit 2
    ;;
  *)
    mkdir -p "$1"
    install "$(cd "$1" && pwd)"
    ;;
esac
