<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

## What changed and why

<!-- Describe the change and the reason for it. Keep it to what a reviewer needs. -->

Closes #NNN

## Checklist

- [ ] Local gates pass: `cargo fmt --all --check`, `cargo clippy ... -D warnings`, `cargo nextest run`, `cargo test --doc`, `cargo doc` (with `RUSTDOCFLAGS=-D warnings`), and `cargo deny check`.
- [ ] The `--self-test` of every `scripts/gh/` helper and `scripts/` guard the change touched passes.
- [ ] A changelog fragment, `changelog.d/<issue>-<kebab-slug>.<section>.md`, records the change if it is user-visible (`changelog.d/README.md`); otherwise the pull request carries the `no-changelog` label.
- [ ] Docs are updated, if behavior changed.
- [ ] Every commit is signed.
- [ ] No AI or assistant attribution anywhere in the commits or this PR.
