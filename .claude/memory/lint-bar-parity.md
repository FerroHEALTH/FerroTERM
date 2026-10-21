---
name: lint-bar-parity
description: The owner's rule (2026-09-04): FerroTERM holds the same lint bar as FerroEHR (~/RustroverProjects/ferroehr); unwrap/expect/panic stay denied in application code and relaxed in tests only; the generated fhir-types crate carries its allow list in the emitter, never by hand
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

FerroTERM's `[workspace.lints]` and `clippy.toml` mirror FerroEHR's (`~/RustroverProjects/ferroehr/Cargo.toml`): `clippy::all` and `pedantic` at `deny`, `as_conversions`, `pub_use`, `dead_code`, `missing_assert_message`, `map_err_ignore`, `unused_qualifications`, `rc_buffer`, `create_dir`, `exit`, the feature-name lints, `non_ascii_idents = forbid`, and the rest (#249). `unwrap`/`expect`/`panic`/indexing are denied in application code and allowed in tests through `clippy.toml` in both repositories; a test's `expect("...")` is within policy.

**Why:** the owner asked "do we allow unwrap in this repo?? our FerroEHR repo is very very strict ... we also need a very very strict code style" and, on finding FerroTERM's table looser, "update that immediately".

**How to apply:** when a lint fights a legitimate case use a scoped `#[expect(lint, reason = "...")]`; a finding inside `fhir-types` is an emitter change in the FerroBRIDGE repository, released and then consumed here (#300). When FerroEHR's table changes, port the change here in the same week. See [[repo-merge-gates]], [[gates-in-container]].
