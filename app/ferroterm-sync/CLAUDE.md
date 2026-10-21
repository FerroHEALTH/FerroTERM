<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# ferroterm-sync

The synchronisation service: one way from a terminology syndication feed into a
running FerroTERM. It runs beside the server, never inside it, and the server
keeps the read path. No FHIR or SNOMED CT specification governs the service;
the feed dialect belongs to `crates/terminology-syndication`.

- `main.rs` stays thin: the command line in, `ferroterm_sync::cli` out. Every
  behaviour lives in `lib.rs` so `tests/it` drives it without a process.
- `anyhow` is allowed in `main.rs` only; the library returns typed errors.
- This is the one workspace member that may depend on `addons/*`
  (`scripts/checks/addon-boundary.sh`). It depends on the syndication client,
  the add-ons, and leaf crates, never on the server or the engine crates, so it
  reads an artifact directory as bytes and a manifest as JSON.

## The rules this service holds

- **A run never returns an error to its caller.** Everything that fails lands
  in the run record, marks the run failed, and still fires the webhook.
- **A failed run leaves the served set exactly as it was.** A build that fails
  never reaches the index root, and a refused reload rolls every move back,
  including the resource file a run replaced.
- **Nothing is deleted except by retention**, and retention runs only after a
  reload succeeded. The server's own write store is never touched.
- **Time comes from the injected `clock::Clock`.** A test drives a whole
  schedule without sleeping, and nothing here reads the wall clock directly.
- **A held content item is never forgotten.** The ledger in the state file is
  what the replace rule compares; an artifact the service did not write is held
  at the latest representable instant, so a run never replaces it.

## Tests

`tests/it/` is one binary with a module per topic. Every test runs against
`wiremock` and a fake build binary the test writes, so no network and no real
build is involved, and the clock is injected. Fixtures are synthetic: invented
entry identifiers, no terminology content.
