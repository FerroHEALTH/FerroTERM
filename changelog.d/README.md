<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Changelog fragments

A pull request with a user-visible effect adds one file here instead of
editing `CHANGELOG.md`, so two pull requests never touch the same lines.

- **Name:** `<issue>-<kebab-slug>.<section>.md`, for example
  `598-changelog-fragments.changed.md`. `<section>` is a Keep a Changelog 1.1.0
  section in lower case: `added`, `changed`, `deprecated`, `removed`, `fixed`
  or `security`.
- **Content:** the entry exactly as it reads in `CHANGELOG.md`, one or more
  list items that open with `- ` and continue on lines indented by two spaces,
  with no heading and no blank line inside. A fragment carries no SPDX header:
  its text moves into `CHANGELOG.md`, which carries the header.
- **Issue reference:** the cut ends the first paragraph of each entry with
  `(#<issue>)`, the number the file name starts with, unless the entry already
  names that issue, so the release notes link every entry to its issue
  without the fragment spelling it out.

## Security entries

A `security` fragment names its advisory itself: the `GHSA-xxxx-xxxx-xxxx` id
of the repository advisory drafted for the fix, or the path of the OpenVEX
statement under `security/vex/` that judges it, which must exist. The cut copies
the fragment's text into the release notes, so the advisory reaches them.

When a release carries `security` fragments beside any other section, its
`### Security` section opens with a reason line (CRA Annex I Part II(2) and
`.claude/rules/changelog.md`). That line is a fragment of its own:

- **Name:** `<issue>-<kebab-slug>.security-reason.md`, at most one at a time.
- **Content:** one paragraph that opens with `Shipped with functional changes
  because: ` and the reason, with no list item and no blank line inside. It is
  copied as written, with no issue reference added.

The cut places it first under `### Security`, before the entries, and deletes
it with the other fragments. A reason with no `security` entry beside it is
refused.

## Checks

`scripts/release/changelog.sh --check` validates every fragment, and CI runs it.
`scripts/release/changelog.sh --preview` prints `CHANGELOG.md` with every
fragment folded into `[Unreleased]`, as the next cut would write it, and
`scripts/release/changelog.sh --check-preview` runs
`scripts/checks/changelog-structure.sh` and
`scripts/checks/changelog-security.sh` over that preview; CI runs it too, so a
Security entry without its advisory, or a missing reason, fails the pull
request that adds it.

The release cut runs `scripts/release/changelog.sh --assemble <version> <date>`,
which moves the fragments into `CHANGELOG.md` under the new version and
deletes them. This README is not a fragment.
