---
name: benchmark-program
description: the owner wants trusted, reproducible benchmarks per served code system (harness #212, README table #213, website redesign #214), run in a container, never hand-typed; the v0.1.0 claim audit is #217
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-09-04 the owner asked for benchmarks like sct-rs's speed table (<https://github.com/pacharanero/sct/blob/HEAD/docs/benchmarks.md>) "but better and more trusted and verified", for every served code system, on the README, with memory beside speed, a website redesign for several audiences with a benchmarks page, and the harness running in a Docker container. Filed as #212 (harness, PR #218), #213 (README table from records), #214 (website redesign); #217 (P0, v0.1.0) audits every public claim against the SNOMED International terminology services expectations and Snowstorm before anything is called stable.

**Why:** the owner's claims must hold up under scrutiny; a number typed by hand or taken on a busy laptop is not a claim.

**How to apply:** numbers on the README, the landing page, the benchmarks page, and the book come only from a committed record set under `bench/records/<date>-<machine>/` (copied by hand from `bench/results/`, one set per machine and run; a set mixing machines is refused) written by `ferroterm-bench` (locally via `bench/run.sh`, or in the container via `docker compose -f bench/compose.yaml run --rm bench`), rendered by `scripts/checks/bench-table.sh render <readme|book|figures|benchmarks>` and checked in CI (`check`); time renders as s, ms, or µs and bytes as GB, MB, or KB by magnitude (the owner asked for this on 2026-09-04); the README record set is taken inside the container (`bench/compose.yaml`), so on a Mac it names the Docker VM; a comparison to Snowstorm is stated only when run on the same machine over the same edition, with the configuration recorded. Related: [[performance-bar]], [[release-cut-cadence]].
