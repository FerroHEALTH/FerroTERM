#!/usr/bin/env bash
# SPDX-FileCopyrightText: Cadasto B.V.
# SPDX-License-Identifier: BUSL-1.1
# run.sh — measure every configured code system into bench/results/.
#
# Builds the release binaries, then runs ferroterm-bench over bench/systems.json
# with the owner-licensed releases and artifacts under data/ and artifacts/
# (never committed). Every argument is passed to ferroterm-bench, e.g.
#   bench/run.sh --skip-ingest --only LOINC
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p ferroterm-server -p ferroterm-build -p ferroterm-bench
exec target/release/ferroterm-bench "$@"
