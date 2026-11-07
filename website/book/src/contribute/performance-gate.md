<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The performance gate

Every pull request that changes what the server or the sync service is
built from runs the `perf` job of CI, a required part of `conclusion`. It
also runs on every push to `main`, and the release workflow runs it on the
release commit before the release is drafted. The job measures the release
builds of `ferroterm` and `ferroterm-sync` with
[`ferrohealth-perf`](https://github.com/FerroHEALTH/FerroSYS/issues/249),
the performance gate every FerroHEALTH server shares. It fails when a
service is busy at rest, or slower, heavier or busier under load than the
committed baseline. `.github/workflows/perf.yml` pins the tool's version.

The gate exists because of
[FerroPIX #490](https://github.com/FerroHEALTH/FerroPIX/issues/490): a
background sender polled its database about 8,000 times a second while it
had nothing to send, and no check caught it before it shipped.

## What it runs

`scripts/perf/gate.sh run` writes the scaled synthetic SNOMED CT edition of
the testkit, 50,000 invented concepts, with `ferroterm-fixture snomed-scaled`,
and starts the server over it and the e2e CodeSystem fixtures, with its
admin listener on. It starts the sync service with a daily schedule whose
next run is twelve hours away. Each service then runs two phases:

1. **Idle.** No request for 60 seconds. The service's CPU time must stay
   under 2 % of one core. The limit is absolute and compared with no
   baseline. Neither service holds a database, so the gate counts no
   transactions.
2. **Load.** 50 requests a second for 60 seconds after a 10-second warm-up,
   open loop. For the server (`perf/server/plan.toml`): `$lookup`,
   `$validate-code` on a code system and on an ECL value set, `$expand` with
   a text filter, two ECL expansions, `$translate` over the implicit ICD-10
   map and `$subsumes`, through the R4 base. For the sync service
   (`perf/sync/plan.toml`): `GET /runs` and the readiness probe. Every
   request must be answered with the status the plan names. Throughput,
   latency p50, p95 and p99 per group, peak memory and CPU time per request
   are compared with the service's `baseline.toml`.

Then the script starts a second sync service whose schedule is
`every = "0s"`, so its scheduler starts each run the moment the last one
ends, and runs the idle phase of `perf/sync/busy-loop.toml` against it. That
run must fail on idle CPU, or the job fails: it proves the gate still
catches a background worker that never waits.

## Reading the report

A load metric that breaches is judged again on up to two fresh load
phases, each after its own warm-up, and the gate fails only when it breaches
in most of the runs, so a one-off stall clears and a real regression fails
every run; the check says in how many runs it breached. A phase where every
route's p99 is over ten times its p50 at once is a stalled runner: it is
reported and not judged, and the run exits 2.

Some runners stall every route for a whole job: on an AMD EPYC 9V74 the
tails reach 100 to 300 ms while the p50 holds
([FerroPIX #511](https://github.com/FerroHEALTH/FerroPIX/issues/511)). An
attempt that ends with exit 1 or 2 on a CPU model the perf workflow lists in
`STALL_PRONE` runs again on a fresh runner, at most twice, and the gate
passes when the last attempt passed. A breach on any other model fails at
once. The job summary names each attempt's CPU model, its verdict and why a
retry ran, and each attempt uploads its reports as `perf-report-attempt-<n>`.

The job summary holds one report per service: the outcome, every breach
with the metric, the baseline, the measured value and the allowed
threshold, every check, and the latency of each group with its 95 %
interval. A metric fails when it is worse than the baseline by more than the
larger of its tolerance (latency and CPU 25 %, throughput 20 %, memory 20 %)
and the noise floor the baseline recorded. The `perf-report-attempt-<n>` artifacts hold
the JSON reports under `target/perf/` with the histograms, so
`ferrohealth-perf compare --report target/perf/server/latest.json --baseline
perf/server/baseline.toml` judges a report again without a server.

The tool exits 0 for a pass, 1 for a breach or a request answered with
another status, and 2 for a run it cannot judge: no baseline, a baseline
written for another plan, a generator that could not hold its rate, or a
window whose second half differs from its first.

## Updating the baselines

GitHub's hosted runners mix CPU generations and memory sizes, and the same
commit measures 25 to 45 % slower on one than on another, so `baseline.toml`
holds one `[[baseline]]` entry per host shape, keyed by the CPU model in
`/proc/cpuinfo` and the memory size to the GiB. A run is judged against the
entry of its runner's shape. A runner whose shape has no entry cannot be
judged: the job exits 2 and names the shape, and the idle limits still judge
the run.

The entries are measured on the CI runners, never on a workstation. An
update measures five load phases, each after its own warm-up, and writes
each metric as their median and its noise floor as how far the worst phase
strayed from it, so a tail that moves from run to run is judged against the
movement it showed. To write or add one, set `update: true` on the `perf`
job's call of `.github/workflows/perf.yml` in `ci.yml` on a branch, take
`perf/server/baseline.toml` and `perf/sync/baseline.toml` from the `perf-
report` artifact, commit them and set `update` back; each file holds the
committed entries and the run's own. Re-run the job until every host shape
the runners serve has its entry. On the same plan an update only moves each
metric of the runner's entry to the better of the entry and the run, and
leaves the other entries as they were, so every entry is a ratchet. A change
to a plan's rate, windows or request set writes the entry afresh. Raising a
metric is a hand edit in a pull request that says why.

Machines of one shape still differ from each other, so re-run the update job
several times and fold the runs into the committed file with
`ferrohealth-perf merge --baseline <file> <job>...`: each entry then takes the median
over the jobs, and its noise floor the spread between them.
