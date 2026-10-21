---
paths: ["scripts/vendor/*.sh", "crates/sct-ecl/vendor/**", "crates/**/data/**", "crates/**/tests/fixtures/**"]
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Vendored inputs and the SNOMED content rule

Two kinds of external material touch this repo, and they are handled
oppositely: a machine-readable corpus is vendored verbatim; SNOMED CT content
is NEVER committed. The HL7 FHIR packages are no longer among them: they are
vendored by the FerroBRIDGE repository, which generates and publishes the
`fhir-types` crate this one consumes (`codegen.md`).

## A vendored corpus: verbatim, pinned, provenance-stamped

Every vendored tree is:

- **Fetched by a committed `scripts/vendor/*.sh` script.** Never hand-download
  into the tree, never hand-edit a vendored file, never paste a corpus in
  from a chat transcript. To refresh or extend: change the script, re-run it,
  commit the result.
- **Vendored verbatim**, byte-for-byte as its publisher ships it.
- **Stamped with a `PROVENANCE.md`** recording the source, the exact version
  or commit pin, the fetch date, and the upstream licence, with the upstream
  `LICENSE` alongside.

| input | script | destination |
|---|---|---|
| The ECL grammar and example corpus (IHTSDO, Apache 2.0), tag pinned in `docs/VERSIONS.md` | `scripts/vendor/ecl-grammar.sh` | `crates/sct-ecl/vendor/` |
| The Compositional Grammar normative ABNF and example corpus (IHTSDO, Apache 2.0), commit pinned in `docs/VERSIONS.md` | `scripts/vendor/scg-grammar.sh` | `crates/sct-scg/vendor/` |
| The IANA, CLDR, and UCUM registry data behind the registry code systems | `scripts/vendor/registries.sh` | `crates/fhir-terminology/data/` |
| Three public terminology syndication listings, the parser's real corpus | `scripts/vendor/syndication-feeds.sh` | `crates/terminology-syndication/vendor/feeds/` |
| The FerroPULSE report envelope v2 JSON Schema and its examples (Cadasto B.V.), commit pinned in `docs/VERSIONS.md` | `scripts/vendor/ferropulse-schemas.sh` | `app/ferroterm-server/tests/fixtures/ferropulse/` |

A vendored tree is protected by discipline and review; a hand-edit of one is a
defect to revert. The pins it names are checked against `docs/VERSIONS.md` by
`scripts/checks/versions.sh`.

## SNOMED CT content is NEVER committed (licence-gated)

SNOMED CT is licensed by SNOMED International (free within member countries,
affiliate licence elsewhere). **The repository ships no RF2 content and no
derived edition data:** no concepts, no descriptions, no relationships, no
transitive-closure file, no built `redb`/`fst`/roaring artifacts derived from a
release. A deployment brings its own licensed RF2 release; `tools/ferroterm-build`
turns it into the served artifacts offline, outside version control.
A developer with a licence keeps a release under `data/` (the whole directory
is gitignored), for example `data/snomed/<release>/Snapshot/`; that is the
local input for `ferroterm-build` and the reference-server comparison.

- **Test fixtures use shaped, synthetic content only:** a small, hand-built
  hierarchy invented for the test (synthetic SCTIDs, invented terms) that
  exercises subsumption, ECL, expansion, and search. **Never** extract real
  SNOMED concepts, descriptions, or terms from a release into a fixture, even
  a handful: that is redistribution of licensed content. The reference-server
  comparison (`testing.md`) runs against a locally-provisioned licensed
  edition that stays out of the repo.
- **`.gitignore` refuses RF2 and built artifacts** so a licensed release
  dropped into a working tree cannot be committed by accident (the
  `Snapshot`/`Full`/`Delta` RF2 layouts, `*.rf2`, `sct2_*`/`der2_*` files, and
  the build output directory). If a genuinely new content shape appears,
  widen `.gitignore` in the same change; never commit content to "just get it
  working".

When in doubt about whether a byte is licensed SNOMED content: it is, and it
does not go in the repository.
