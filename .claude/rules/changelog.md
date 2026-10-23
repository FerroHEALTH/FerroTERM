---
paths:
  - "CHANGELOG.md"
  - "changelog.d/**"
  - "SECURITY.md"
  - ".github/workflows/release*.yml"
  - "scripts/release/**"
  - "scripts/checks/changelog-*.sh"
  - "scripts/checks/release-notes.sh"
---

# Changelog discipline (Keep a Changelog 1.1.0)

`CHANGELOG.md` is the release record (<https://keepachangelog.com/en/1.1.0/>
plus SemVer), and the release notes are cut from it. A change with a
user-visible effect adds a fragment under `changelog.d/` (the format is
`changelog.d/README.md`); the cut assembles the fragments into the new version's
section with `scripts/release/changelog.sh --assemble`. The procedure of the
cut is `docs/release.md`.

- **Six section types, each at most once per release section:** `Added`,
  `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`.
  `scripts/checks/changelog-structure.sh` refuses any other heading and a
  duplicate one.
- **Write for the person who runs FerroTERM:** what changed for them, and the
  upgrade step when a change breaks something.

## Security fixes and advisories

Manufacturer duties under the Cyber Resilience Act (Regulation (EU) 2024/2847,
Annex I Part II), checked on every pull request and again at the cut.
`scripts/checks/changelog-security.sh` checks both rules below: CI runs it over
the assembled `[Unreleased]` preview (`scripts/release/changelog.sh
--check-preview`), and the release `plan` job over the section being cut.
`scripts/release/preflight.sh vX.Y.Z` runs every check of the plan job before
the tag (`docs/release.md`).

- **A security fix ships alone, or says why not** (Annex I Part II(2): "where
  technically feasible, new security updates shall be provided separately
  from functionality updates"; `SECURITY.md` §Supported versions). A release
  whose `### Security` entries sit beside any other change subsection is
  refused unless its `### Security` section opens with a line
  `Shipped with functional changes because: <reason>`, naming why a
  security-only patch was technically infeasible (the fix is only correct
  with a behavioural change; the code it fixes was already replaced on
  `main`). In this repository the line is the one
  `changelog.d/*.security-reason.md` fragment, which the cut places first
  under `### Security`. The release notes are extracted from that section, so
  the reason reaches them. "The next release was due anyway" is not a reason:
  cut a security-only patch from the last release's state instead.
- **Every `### Security` entry names its advisory** (Annex I Part II(4) and
  (8); `SECURITY.md` §Reporting a vulnerability). Each entry carries the
  `GHSA-…` identifier of the repository advisory published with the release,
  or, for a dependency bump that the OpenVEX documents show does not affect
  any shipped artefact, the path of that VEX statement under `security/vex/`,
  which must exist. An entry with neither refuses the cut. Publishing the
  drafted advisories is a step of the cut, in the same hour the GitHub release
  is published (`docs/post-market.md`, "A fixed vulnerability's advisory").

Released sections are never checked retroactively: the two rules read only
`[Unreleased]` and the section being cut.

## The release notes

The `plan` job writes the notes with `scripts/release/plan.sh notes`: the
changelog section, then the support period (the end month, five years from the
date on the section's heading; CRA Art 13(8) and (19)) and the link to the CRA
information and instructions to the user in the book (Art 13(18), Annex II).
`scripts/checks/release-notes.sh` refuses notes without either, so a section
heading without a date stops the release.
