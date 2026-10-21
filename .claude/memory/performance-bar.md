---
name: performance-bar
description: "The owner's performance expectations, point reads under 1 ms and the NL edition ingest under 60 s, measured, never called \"fine\" at a higher number"
metadata: 
  node_type: memory
  type: feedback
  originSessionId: 1fab17b9-946d-49fc-af2e-4719ce97bc31
  modified: 2026-09-02T21:12:39.015Z
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every point read and every terminology operation answers in under 1 ms in
the release profile on the NL edition; `ferroterm-build` ingests the NL
edition in under 60 s. A latency in the milliseconds is never described as
fine; it is measured with `criterion` on the built artifact and either meets
the bar or gets an issue with a profile (tracked as #77, 2026-09-02).

**Why:** On 2026-09-02 the owner replied to the #46 comment that called a
12.8 ms open-plus-three-reads figure fine: "what do you mean 10ms is fine we
always need to look if we can make it below 1ms right and for injeest it
should be 1min right if possible that would be amazing".

**How to apply:** Quote numbers with their profile (debug or release) and
what they include; benchmark before judging; treat every millisecond on the
read path as work to do. See [[release-cut-cadence]] and
[[owner-work-style]].
