<!-- SPDX-License-Identifier: BUSL-1.1 -->

## What changed and why

<!-- Describe the change and the reason for it. Keep it to what a reviewer needs. -->

Closes #NNN

## Licensing of contributions

- [ ] I accept the terms in [CONTRIBUTING.md § Licensing of contributions](../CONTRIBUTING.md#licensing-of-contributions): I have the right to submit this work, I license it under the project licence of the version it lands in, and I grant the Licensor the relicensing right stated there.

## Checklist

- [ ] Local gates pass: `cargo fmt --all --check`, `cargo clippy ... -D warnings`, `cargo nextest run`, `cargo test --doc`, `cargo doc` (with `RUSTDOCFLAGS=-D warnings`), and `cargo deny check`.
- [ ] The `--self-test` of every `scripts/gh/` helper and `scripts/` guard the change touched passes.
- [ ] A changelog fragment, `changelog.d/<issue>-<kebab-slug>.<section>.md`, records the change if it is user-visible (`changelog.d/README.md`); otherwise the pull request carries the `no-changelog` label.
- [ ] Docs are updated, if behavior changed.
- [ ] Every commit is signed.
- [ ] No AI or assistant attribution anywhere in the commits or this PR.

Contributions carry the licensing terms in
[CONTRIBUTING.md](../CONTRIBUTING.md#licensing-of-contributions); the
`contribution-licence-guard` check reads the box above, and there is no separate
agreement to sign. See [CONTRIBUTING.md](../CONTRIBUTING.md) for the full
contribution guide.
