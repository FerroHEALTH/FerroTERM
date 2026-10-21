<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferroterm-testkit

Synthetic fixtures shared by the test suites (a dev-dependency only, never a
runtime dependency, `publish = false`). It lives under `tools/` because it is
tooling, not product; a crate's tests may depend on it, which is the one
sanctioned edge from `crates/` into `tools/` (`CLAUDE.md`, the repo map).

- Everything here is invented: shaped like a real edition, with identifiers in
  an invented namespace carrying valid check digits, and terms that name
  nothing clinical. No SNOMED CT content, ever (`.claude/rules/vendored-inputs.md`).
- A fixture writes what the offline build would write, through the store,
  graph, and text writers, so a consumer test exercises the real layout.
- Fixture code follows the library lints; test-only relaxations stay in the
  test binaries that use it.
