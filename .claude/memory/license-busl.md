---
name: license-busl
description: the project's own code is under the Business Source License 1.1 on FerroEHR's terms since 2026-09-04 (#221), after MIT, a first BUSL change (#147), and an Apache 2.0 interlude (#150); the versions guard fails on stale MIT or Apache claims
metadata:
  type: project
---

Licence history: MIT until 2026-09-03; BUSL 1.1 (#147) merged and was reverted for Apache 2.0 the same day (#150); on 2026-09-04 the owner asked, as priority 0, for BUSL 1.1 again "like what we have for FerroEHR" (#221): non-commercial production use is free, commercial production use needs a licence from the Licensor, hosted/managed/embedded services and for-fee distribution always need one, Apache 2.0 four years after each version. The LICENSE is FerroEHR's text with the Licensed Work renamed and clause (a) extended to terminology and code system content.

**Why:** the owner wants FerroTERM and FerroEHR under one licensing model; the earlier revert to Apache was undone.

**How to apply:** every header (`SPDX-License-Identifier: BUSL-1.1`), manifest, badge, page, image label, and crate LICENSE copy names BUSL-1.1; `scripts/checks/versions.sh` fails on a stale MIT or Apache-2.0 claim in the project's own files. Twelve published crates carry BUSL-1.1; `fhir-types` and `rf2` stay Apache 2.0 (the owner chose this on 2026-09-04, #223), as FerroEHR keeps its spec crates open; the versions guard checks both directions. Licensor and copyright holder: Cadasto B.V. since 2026-10-05 (#700), and Cadasto also handles the business side, the commercial licence included (#702): every LICENSE copy, the README, the handbook licensing page, MAINTAINERS.md, GOVERNANCE.md and SUPPORT.md send commercial and business questions to info@cadasto.com or https://www.cadasto.com/contact/, never to the maintainer. The maintainer (Ruben Talstra) keeps the technical side: code, review, releases, issues, security reports. Related: [[crate-names-and-publishing]], [[release-cut-cadence]].
