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
build of its base, measured beside it on the same runner in the same job.
`.github/workflows/perf.yml` pins the tool's version.

The gate exists because of
[FerroPIX #490](https://github.com/FerroHEALTH/FerroPIX/issues/490): a
background sender polled its database about 8,000 times a second while it
had nothing to send, and no check caught it before it shipped.

## Why the head is judged against its base

GitHub's hosted runners differ: two runners that report the same CPU model
and memory measured one commit 25 to 45 % apart in p50 latency and CPU per
request
([FerroEHR #5095](https://github.com/FerroHEALTH/FerroEHR/issues/5095)).
A baseline stored from one machine failed unchanged code on another, so the
gate keeps none. It builds the base of the change and the change itself,
the head, runs both on one runner, and judges the head over the base. The
difference between machines reaches both alike and cancels.

The base is:

- for a pull request, the merge base of the commit CI checks out with the
  pull request's base branch;
- for a merge group, its base on `main`;
- for a push to `main`, the commit before the push;
- for the release commit and a manual dispatch, the previous release tag.

## What it runs

`scripts/perf/gate.sh build` builds the release server, sync service and
fixture writer of the base, from `git archive` of the base commit under
`target/perf-base/`, and of the head. A base must define the server and the
sync service; a base older than the fixture writer, such as v0.4.2, builds
the two services alone. `scripts/perf/gate.sh run` writes the
scaled synthetic SNOMED CT edition of the testkit, 50,000 invented
concepts, with each side's `ferroterm-fixture snomed-scaled`, the head's for
a base without one, and starts
each side's server over its own edition and its own e2e CodeSystem
fixtures, with its admin listener on: the head on ports 8080 and 8081, the
base on 8090 and 8091. It starts each side's sync service against its own
server with a daily schedule whose next run is twelve hours away, the head
on 8181 and the base on 8191. Then it runs `ferrohealth-perf ab` once per
service:

1. **Idle.** No request for 60 seconds. The head's CPU time must stay under
   2 % of one core. The limit is absolute and compared with nothing.
   Neither service holds a database, so the gate counts no transactions.
2. **Load, in pairs.** Four pairs of load phases, each phase 50 requests a
   second for 60 seconds after a 10-second warm-up, open loop, against one
   side. The two phases of a pair run at once, the arrivals of one side half
   an interval after the other's, so both sides meet the runner in the same
   state; run one after the other, the runner's speed moved by up to a third
   between phases and put an unchanged head up to a third off its base
   (#1162). For the server (`perf/server/plan.toml`):
   `$lookup`, `$validate-code` on a code system and on an ECL value set,
   `$expand` with a text filter, two ECL expansions, `$translate` over the
   implicit ICD-10 map and `$subsumes`, through the R4 base. For the sync
   service (`perf/sync/plan.toml`): `GET /runs` and the readiness probe.
   Every request must be answered with the status the plan names.

Then the script starts a head sync service whose schedule is
`every = "1ms"`, the shortest interval the service accepts, so its
scheduler starts each run the moment the last one ends, and runs the idle phase of `perf/sync/busy-loop.toml` against it. That
run must fail on idle CPU, or the job fails: it proves the gate still
catches a background worker that never waits.

## Reading the report

Each load metric (throughput, latency p50, p95 and p99 per group, peak
memory and CPU time per request) is the median of a side's four phases.
Its noise floor is the worse of the two sides' spread, how far a side's
worst phase strayed from its median. A metric breaches when the head's
median is worse than the base's by more than the larger of its tolerance
and that floor; the tolerances are each plan's `[tolerance]` table: latency
and CPU 25 %, throughput 20 %, memory 20 %. It fails only when the head also
breaches against the base in most of the pairs, so a stall in one phase
clears and a regression, which repeats in every pair, fails.

The job summary holds one report per service: the outcome, the base
commit, the head's idle limit, and per metric the base, the head, their
ratio, the tolerance, the floor, the threshold and in how many pairs the
head breached. Requests the head answered with another status are listed
and fail the gate; those the base answered with another status, such as a
route the change adds, are notes, and a metric only one side measured is
noted and not judged. Each attempt uploads its reports as
`perf-report-attempt-<n>`: the JSON reports under `target/perf/` with every
phase's histograms, the four services' logs and the base commit, so
`ferrohealth-perf compare --report target/perf/server/latest.json` judges a
report again without a server.

The tool exits 0 for a pass, 1 for a breach or a request answered with
another status, and 2 for a run it cannot judge: a phase whose generator
could not hold its rate, whose second half differs from its first, or that
stalled, and a base that fell behind the schedule.

## Retries

Some runners stall every route for a whole job: on an AMD EPYC 9V74 the
tails reach 100 to 300 ms while the p50 holds
([FerroPIX #511](https://github.com/FerroHEALTH/FerroPIX/issues/511)). A
phase where every route's p99 is over ten times its p50 at once is a
stalled runner: it is reported, left out of the judgement, and the run
exits 2. An attempt that ends with exit 2 runs again on a fresh runner on
any CPU model, since a run that cannot be judged says nothing about the
code. An attempt that ends with exit 1 runs again only on a CPU model the
perf workflow lists in `STALL_PRONE`. At most two retries run, and the gate
passes when the last attempt passed. A breach on any other model fails at
once. The job summary names each attempt's CPU model, its verdict and why
a retry ran.

## Changing a plan or the tolerances

The plans are the head's: a change to `perf/server/plan.toml` or
`perf/sync/plan.toml` is measured on both sides in the same pull request. A
tolerance moves only in a pull request that says why.
