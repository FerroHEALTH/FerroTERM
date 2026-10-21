---
name: owner-work-style
description: How the owner wants FerroTERM work done (research-first, evidence-based, from-first-principles, confirm foundational decisions before scaffolding)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On FerroTERM, the owner decides foundational architecture from **cited research
(academic papers included), not convention**, and is explicitly skeptical of the
"legacy" way others build CT/terminology servers (Elasticsearch,
everything-in-RAM, general-purpose search engines).

**Why:** the foundation is treated as the most important thing to get right from
the start. **How to apply:**
- For any foundational choice, do proper research and put the evidence in front
  of the owner BEFORE building. Present options with a recommendation; do not
  default to the conventional answer.
- Do NOT scaffold code/workspace while the design is still open; the owner
  stopped an early scaffold saying "still in the discover phase". Build only
  after the direction is confirmed.
- Take the owner's design intuitions seriously and test them against evidence:
  the graph model was the owner's call and is correct; the research refined HOW
  to implement it (materialized index, not a graph database). Reconcile, don't
  dismiss.
- Pure Rust, memory-safe, lightweight, single binary are standing constraints.

Same defer-nothing, proper-rewrites-welcome spirit as on FerroEHR. See
[[architecture-decisions]].
