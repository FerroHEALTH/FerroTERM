---
name: worker-cap-ten
description: From 2026-11-03 up to 10 implementation workers may run at once across the FerroHEALTH repositories, replacing the family cap of 5 set on 2026-10-19
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Up to 10 implementation workers may run at once across the FerroHEALTH repositories. This replaces the family cap of 5 set on 2026-10-19 (and the earlier caps of 2 and 4).

**Why:** the owner, 2026-11-03: "you are allowed to have 10 workers okay!!", after asking why so few workers ran while every product had open work.

**How to apply:** keep the slots filled with real work across all products. Give each worker disjoint files, so no two edit the same files at once; work in one module goes to one worker in sequence ([[pr-backlog-discipline]]). Read-only analysis agents count toward the 10 when they run long.
