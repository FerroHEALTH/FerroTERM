<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Contributing to FerroTERM

FerroTERM is a pure-Rust FHIR terminology server for SNOMED CT, LOINC, and other
clinical code systems, SNOMED CT first. It is in early
design; the architecture is recorded, with citations, in
[`docs/architecture.md`](docs/architecture.md), and the working discipline is in
[`CLAUDE.md`](CLAUDE.md). Read both before making a change.

## The two layers

- The FHIR model is the **generated** `fhir-types` crate, taken from
  crates.io. Its generator and the machine-readable HL7 packages behind it live
  in the FerroBRIDGE repository, so a missing or wrong shape is fixed and
  released there, never re-modelled here.
- The SNOMED engine (`rf2`, `concept-graph`, `concept-store`, `designation-index`,
  `sct-ecl`, `fhir-terminology`) and the server (`app/ferroterm-server`) are
  **hand-written**, modern idiomatic Rust; the FHIR and SNOMED specifications
  are the authority.

## Build and test

Once the Cargo workspace exists, the local gates mirror CI exactly:

```
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo nextest run --workspace --locked
cargo test --doc --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --document-private-items
cargo deny check
```

Run the same commands scoped to a crate (`-p <crate>`) while iterating; run the
full `--workspace` gates before opening a pull request. All cargo invocations
use `--locked`; `Cargo.lock` is committed.

The workflow lanes that also run on shell and YAML (`actionlint`, `zizmor`,
`shellcheck`) run on every change, including before any Rust exists. If you edit
a shell script, keep it clean at `shellcheck --severity=style`.

## Branches and commits

- Branch names use conventional types: `feat/<slug>`, `fix/<slug>`,
  `chore/<slug>`, `docs/<slug>`, `refactor/<slug>`, `perf/<slug>`, `test/<slug>`,
  `ci/<slug>`, `build/<slug>`. Never force-push `main`.
- Commit messages describe only the change. Do **not** add AI/assistant
  attribution, co-author trailers, or "generated with" lines anywhere.
- **Sign your commits.** Configure commit signing (GPG, SSH, or S/MIME) so every
  commit is verified.

## Pull requests

- Keep changes compiling and tested at every step; do not defer compilation.
- Never weaken, skip, or delete a test to make a build pass.
- Update the changelog for any user-visible change once `CHANGELOG.md` exists.
- Every workflow `uses:` is pinned to a full commit SHA with a trailing version
  comment; keep it that way. `permissions:` is `{}` at the workflow level with
  the minimum granted per job, and no untrusted context is interpolated into a
  `run:` block; pass it through `env:`.

## Security

Report vulnerabilities privately. See [`SECURITY.md`](SECURITY.md). Do not open
a public issue for a security problem.

## Licensing of contributions

FerroTERM's own code is licensed under the Business Source License 1.1
([`LICENSE`](LICENSE)). By submitting a contribution you:

1. certify that you wrote it, or otherwise have the right to submit it under
   these terms;
2. license it under the Business Source License 1.1 as applied to the version it
   lands in, including that version's Change License, so it becomes Apache 2.0
   with the rest of that version; and
3. grant the Licensor named in `LICENSE`, Cadasto B.V., a perpetual,
   irrevocable, worldwide, royalty-free, transferable right to use, reproduce,
   modify, distribute, sublicense and relicense the contribution as part of the
   Licensed Work under any terms, including commercial licences.

You keep your copyright. Point 3 is what lets the Licensed Work stay one work
with one licensor: a commercial licence, a change of the licence parameters, or a
transfer of the project can then cover every line, not only the maintainer's own.
There is no separate agreement to sign: the pull request template carries a
checkbox recording your acceptance of these terms, and a pull request from a
person does not merge without it (the `contribution-licence-guard` check, backed
by `scripts/checks/contribution-licence.sh`).
