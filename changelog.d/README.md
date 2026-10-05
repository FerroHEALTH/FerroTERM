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

`scripts/release/changelog.sh --check` validates every fragment, and CI runs it.
The release cut runs `scripts/release/changelog.sh --assemble <version> <date>`,
which moves the fragments into `CHANGELOG.md` under the new version and
deletes them. This README is not a fragment.
