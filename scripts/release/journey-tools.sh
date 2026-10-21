#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# Installs the browser the console's journeys drive, Chrome for Testing and
# its matching chromedriver, at the release docs/VERSIONS.md pins (the one the
# `ui-e2e` job of ci.yml installs), each zip checked against its pinned SHA-256
# before it is unpacked (no specification governs this: our own design).
#
# The download URLs come from the Chrome for Testing JSON API, for the one
# pinned version and this platform. scripts/checks/versions.sh holds the
# version below to docs/VERSIONS.md and to the chrome-version of ci.yml.
# scripts/ui-e2e.sh runs this into target/journey-tools when neither
# FERROTERM_UI_E2E_CHROMEDRIVER nor PATH names a chromedriver.
#
# Usage:
#   scripts/release/journey-tools.sh DIR   install both into DIR, as
#                                          DIR/chrome-<platform>/ and
#                                          DIR/chromedriver-<platform>/
#   scripts/release/journey-tools.sh --paths DIR
#                                          print the chromedriver and the
#                                          Chrome binary DIR holds, one per
#                                          line, for this platform
set -Eeuo pipefail

chrome_version="155.0.8059.79"
chrome_linux64_sha256="bcd9a79130d7e44d70f93be87042e9bcee7fe0f0b3898e06b8c9bed5fb619dcd"
chrome_mac_arm64_sha256="e141a7cbf948780bb7d381860bd5a7944c6736f79b9918b9826fccb199d28ea4"
chromedriver_linux64_sha256="876f0ea4095237bb3e93321d95758d38c9195989935d7bdc9f51d8e26bc6a3a1"
chromedriver_mac_arm64_sha256="ca9a98c74b8d3904be6536ff6d26493abb7d7a476e2b384a75cd55d101bce29d"

api="https://googlechromelabs.github.io/chrome-for-testing/known-good-versions-with-downloads.json"

die() {
  echo "journey-tools: $*" >&2
  exit 1
}

# Prints the SHA-256 of FILE, with the tool the platform has.
digest_of() {
  local file="$1"
  if command -v sha256sum > /dev/null; then
    sha256sum "$file" | cut -d' ' -f1
  else
    shasum -a 256 "$file" | cut -d' ' -f1
  fi
}

# Checks FILE against the SHA-256 DIGEST.
verify() {
  local file="$1" digest="$2" found
  found="$(digest_of "$file")"
  [[ "$found" = "$digest" ]] || die "$file has SHA-256 $found, the pin is $digest"
}

# Downloads URL into FILE.
fetch() {
  local url="$1" file="$2"
  curl --fail --silent --show-error --location --retry 3 --output "$file" "$url"
}

# Prints the URL the API lists for the pinned version of PRODUCT (chrome or
# chromedriver) on PLATFORM, from the API answer in FILE.
url_of() {
  local product="$1" platform="$2" file="$3"
  jq -r --arg version "$chrome_version" --arg product "$product" --arg platform "$platform" \
    '.versions[] | select(.version == $version) | .downloads[$product][]?
      | select(.platform == $platform) | .url' "$file"
}

# Fetches PRODUCT for PLATFORM into DIR, checks it against DIGEST, and
# unpacks it, replacing an earlier copy.
install_one() {
  local product="$1" platform="$2" digest="$3" dir="$4" url zip
  url="$(url_of "$product" "$platform" "$dir/download/versions.json")"
  [[ -n "$url" ]] || die "the Chrome for Testing API lists no $product $chrome_version for $platform"
  zip="$dir/download/$product-$platform.zip"
  fetch "$url" "$zip"
  verify "$zip" "$digest"
  rm -rf "${dir:?}/$product-$platform"
  unzip -q "$zip" -d "$dir"
}

# Prints this platform's name in the API and the two binaries under DIR.
platform_paths() {
  local dir="$1"
  case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)
      printf '%s\n%s\n%s\n' linux64 "$dir/chromedriver-linux64/chromedriver" "$dir/chrome-linux64/chrome"
      ;;
    Darwin-arm64)
      printf '%s\n%s\n%s\n' mac-arm64 "$dir/chromedriver-mac-arm64/chromedriver" \
        "$dir/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing"
      ;;
    *) die "no pinned digest for $(uname -s) $(uname -m); install Chrome for Testing $chrome_version by hand" ;;
  esac
}

install() {
  local dir="$1" platform chrome_sha driver_sha driver chrome said paths
  paths="$(platform_paths "$dir")"
  platform="$(sed -n 1p <<<"$paths")"
  driver="$(sed -n 2p <<<"$paths")"
  chrome="$(sed -n 3p <<<"$paths")"
  case "$platform" in
    linux64)
      chrome_sha="$chrome_linux64_sha256"
      driver_sha="$chromedriver_linux64_sha256"
      ;;
    *)
      chrome_sha="$chrome_mac_arm64_sha256"
      driver_sha="$chromedriver_mac_arm64_sha256"
      ;;
  esac
  command -v jq > /dev/null || die "jq reads the Chrome for Testing API; install it first"
  command -v unzip > /dev/null || die "unzip unpacks the downloads; install it first"
  mkdir -p "$dir/download"
  fetch "$api" "$dir/download/versions.json"
  install_one chrome "$platform" "$chrome_sha" "$dir"
  install_one chromedriver "$platform" "$driver_sha" "$dir"
  said="$("$driver" --version)"
  case "$said" in
    *"ChromeDriver $chrome_version "*) ;;
    *) die "the unpacked chromedriver says '$said', not $chrome_version" ;;
  esac
  [[ -x "$chrome" ]] || die "the Chrome zip holds no $chrome"
  echo "journey-tools: Chrome for Testing and chromedriver $chrome_version are in $dir"
}

case "${1:-}" in
  --paths)
    [[ $# -eq 2 ]] || { echo "usage: scripts/release/journey-tools.sh --paths DIR" >&2; exit 2; }
    platform_paths "$2" | sed 1d
    ;;
  "" | -*)
    echo "usage: scripts/release/journey-tools.sh DIR | --paths DIR" >&2
    exit 2
    ;;
  *)
    mkdir -p "$1"
    install "$(cd "$1" && pwd)"
    ;;
esac
