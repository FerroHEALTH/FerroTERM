<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferroterm-build

The offline build: a licensed RF2 release in, the served artifacts out, once
per edition. Hand-written tooling; the RF2 release file specification governs
the input, and the store, graph, and text crates own their formats.

- Synchronous and offline by design; the async and non-blocking discipline is
  the server's, not this tool's (`.claude/rules/reliability.md`).
- Determinism is a requirement: two builds of the same release produce
  byte-identical artifacts.
- The build reads and writes on several threads (`rayon`), and every parallel
  stage is ordered by construction: a per-file read collects into a position
  keyed structure and is joined in path order, and each artifact is written
  from its own inputs alone. Never let a result depend on which worker
  finished first.
- Peak resident memory lands in the write phase, where the rows the read pass
  produced, the rows `StoreBuilder` buffers for its one transaction, and the
  designation index under construction are alive together. The store buffer
  stays for the reason recorded on #338; a builder that writes rows through as
  they arrive is what lowers the peak.
- The output directory is never committed (`.gitignore` refuses `*.redb`,
  `*.fst`, and `/artifacts/`); neither is any RF2 input
  (`.claude/rules/vendored-inputs.md`).
- `main.rs` is thin over `lib.rs`; the pipeline is tested end to end over a
  synthetic RF2 fixture.
- This crate is a tool, so it may write to stdout and stderr; every such site
  carries a scoped `#[expect]` with a reason.
