---
paths: ["crates/**", "scripts/release/**", ".github/workflows/publish-crates.yml"]
---

# Published crates discipline (crates.io)

The `crates/*` members are published on crates.io under plain names
(`rf2`, `concept-graph`, `concept-store`, `designation-index`,
`sct-ecl`, `sct-scg`, `fhir-terminology`, `loinc`, `classification`, `dhd-thesaurus`,
`gstandaard`, `labcodeset`, `icd11`, `rxnorm-rrf`; the owner's decision on
#164, 2026-09-03, `labcodeset` added 2026-09-04, `sct-scg` added 2026-09-23
and published once by hand first, because the lane cannot create a new
crate) so other projects can depend on them. The server (`app/*`), the tools
(`tools/*`), and the add-ons (`addons/*`) are never published.
`crates/terminology-syndication` is the one `crates/*` member that is not
published either (the owner's decision, 2026-09-23): it says `publish = false`
with its `# NOTE: unpublished:` reason and sits outside the `CRATES` list,
while its version still moves with the lockstep line because it lives under
`crates/*`. Every member is BUSL-1.1 except `rf2`, which
is Apache 2.0 (the owner's decision on #223, 2026-09-04) so the RF2 reader
stays usable by any Rust project. `fhir-types` left this line with the FHIR
model (#300) and is published by the FerroBRIDGE repository. Published versions are immutable, so version
hygiene is a hard rule, machine-enforced by the `crate-version-guard` CI job
and the push hook `.claude/hooks/crate_version_bump_guard.sh`.

## Two version lines

- **The product version** is the workspace `version` in the root `Cargo.toml`
  (the server, the tools, the release tag `vX.Y.Z`).
- **The crate line** is the `version` in each `crates/*/Cargo.toml`, the same
  `0.x` in every member and in every internal requirement in the root
  `[workspace.dependencies]` table. It never adopts the product version or a
  spec version; it is the crates' own SemVer line.

## The bump rule

- **A PR that changes any packaged content of any `crates/*` member bumps the
  crate line in the same PR.** Packaged content is what the crate's `include`
  ships: `src/**`, `data/**`, `README.md`, `LICENSE`, and `Cargo.toml`. Tests,
  benches, `CLAUDE.md`, and the vendored grammar are not packaged and need no
  bump. A root `[workspace.dependencies]` entry a member consumes is packaged
  content too, because `cargo package` renders the concrete requirement.
- **Bumps are lockstep:** every member's `version` and every internal
  requirement move to the same new `0.x` in one sweep, and `Cargo.lock` is
  refreshed in the same PR (`cargo update -w`). The guard fails a split bump
  and a stale lock.
- Escape: the `no-crate-bump` PR label, only when the diff provably alters no
  packaged bytes. Locally, `FERROTERM_SKIP_CRATE_BUMP_GUARD=1 git push`.
- Not every bumped version is published; gaps in the published sequence are
  normal. Publishing different content under an existing version is what is
  forbidden, and crates.io refuses it.

## The publish lane is per crate, resumable, and verified

`scripts/release/publish-crates.sh` (`publish` / `verify` / `version`)
uploads the members one at a time in dependency order, counts "already exists"
as done, and reads the registry back before reporting success. Two lanes call
it: the `crates` leg of `release.yml` on a `v*` tag (the primary path, paused
by the `crates-io` environment's required reviewer) and `publish-crates.yml`
on a manual dispatch (a dry run by default; `publish = true` is the recovery
path). Both authenticate with crates.io Trusted Publishing (OIDC through
`rust-lang/crates-io-auth-action`); no long-lived crates.io token exists in
the repository. Neither lane restores a build cache (`ci-cd.md`).

## Owner steps (once per crate, and once for the environment)

1. The first-ever version of a crate cannot use Trusted Publishing: the owner
   runs `cargo login` locally and `scripts/release/publish-crates.sh publish`
   from the merged `main`, with `scripts/release/publish-crates.sh verify`
   after it.
2. On crates.io, each crate's Settings, Trusted Publishing: two GitHub entries,
   repository owner `FerroHEALTH`, repository `FerroTERM`, workflow
   `release.yml` and workflow `publish-crates.yml`, environment `crates-io`.
3. The `crates-io` GitHub environment carries the owner as required reviewer.

## Before publishing: the C-STABLE adjudication

`reliability.md` deviates from C-STABLE while the crates are unpublished. The
pre-1.0 dependencies that appear in a published crate's public API (`redb`,
`roaring`, `fst`, `winnow`, `logos`, `jiff`) are adjudicated on #164 before the
first publish, and again whenever the line graduates past `0.x`.

## The one published list

The `CRATES` array in `scripts/release/publish-crates.sh` is the published
list, in upload order. `scripts/checks/versions.sh` reads it: a `crates/*`
member in the list needs README, LICENSE, `publish = true`, and the lockstep
version; a member outside it must say `publish = false` and carry a
`# NOTE: unpublished: <reason>` line in its manifest. A name in the list with
no member, or a member in neither state, fails the check.
