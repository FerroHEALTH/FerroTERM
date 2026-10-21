---
name: e2e-own-workspace
description: "e2e/ is its own Cargo workspace, so root `cargo fmt --all` and `cargo clippy` never touch it; run both inside e2e/ before pushing"
metadata: 
  node_type: memory
  type: project
  originSessionId: f1d07f84-66e2-401d-8ebd-76aea8bebf4f
  modified: 2026-09-08T19:16:18.983Z
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

`e2e/` carries its own `Cargo.toml` workspace and its own `Cargo.lock`, so
`cargo fmt --all` and `cargo clippy` run from the repository root do not see
`e2e/tests/it/*.rs` at all. CI's `ui-e2e` job runs them inside `e2e/` and fails
on the difference.

**Why:** the failure surfaces only in CI, a full round trip after the push, and
it reads as an unrelated "The journeys hold the same formatting and lint bar"
step failure rather than as a formatting slip.

**How to apply:** after touching anything under `e2e/tests/`, run
`cd e2e && cargo fmt --all && cargo clippy --tests --all-features -- -D warnings`
in addition to the root gates. Related: [[repo-merge-gates]].
