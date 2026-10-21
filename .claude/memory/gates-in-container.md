---
name: gates-in-container
description: Every gate, tests included, runs on the host CLI; the pinned-container test run is retired because its target volume grew to 104 GB and crashed Docker
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Run every gate on the host: `cargo fmt`, `cargo clippy --workspace --all-targets --all-features`, `RUSTDOCFLAGS="-D warnings" cargo doc`, `scripts/checks/*.sh`, **and `cargo nextest run --workspace --locked`**.

The pinned `rust:1.98-bookworm` container run is retired (the owner's instruction, 2026-09-05). Its `ferroterm-gates-target` volume reached 104.4 GB and crashed Docker once; both volumes were removed. Do not recreate them and do not run the suite in Docker.

**Why:** the container existed because macOS Gatekeeper stalls a freshly built unsigned test binary for about four minutes on first execution. That cost is the owner's to accept; a runaway 100 GB cache is not. A host run pays the Gatekeeper wait occasionally and nothing else.

**How to apply:** run `cargo nextest run --workspace --locked` in the working copy. If a run seems to hang before any test prints, that is Gatekeeper on the fresh binary; wait it out rather than switching tooling. See [[perl-edit-pitfalls]] for the other local-run trap.
