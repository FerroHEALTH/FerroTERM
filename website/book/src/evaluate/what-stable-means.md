<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# What stable means in v0.1.0

v0.1.0 is the first release the project calls stable. Stable here is a promise
about specific surfaces, not about all of them, so this page says which ones
carry it and which do not. A surface that carries the promise changes only in
ways your deployment survives; a surface that does not may change in any
release, and this page is where you find out which is which.

The version itself is still `0.x`, and it stays there until the surfaces below
have been through real deployments. Read the promise, not the number.

## What carries the promise

**The HTTP terminology API for R4, R4B, and R5.** The operations
(`$lookup`, `$validate-code`, `$subsumes`, `$expand`, `$translate`), the
parameters each version's `OperationDefinition` declares, the shape of the
`Parameters` answer, and the `OperationOutcome` a refusal carries. A request
that works against v0.1.0 works against every later 0.1.x. Where the
specification and this server disagree, the specification wins and the change
is a fix, which the [conformance page](conformance.md) records.

**The URL layout.** The version prefix (`/r4`, `/r4b`, `/r5`), the resource
paths under it, `/metadata` and `/metadata?mode=terminology`, and `/health`.

**The implicit value sets and concept maps of SNOMED CT.** The `?fhir_vs`
forms and `?fhir_cm`, as the
[SNOMED CT URI standard](https://docs.snomed.org/snomed-ct-specifications/snomed-ct-uri-standard)
defines them.

**Configuration.** The `FERROTERM_*` environment variables keep their names
and meanings. New ones may appear; an existing one does not change what it
does, and one that is retired keeps working for the rest of the 0.1 line.

**The licence terms.** BUSL 1.1 with the change date and change licence each
release states. A released version's terms never change after the fact.

## What does not carry the promise

**R6, which is ballot-tracking.** `/r6` answers in the shapes of
`hl7.fhir.r6.core` 6.0.0-ballot5. R6 is not published, so its shapes can
change under us, and when the ballot moves this endpoint moves with it inside
the 0.1 line. Build against `/r5` if you need the promise;
[FHIR versions](../integrate/fhir-versions.md) has the detail.

**The built artifact format.** The layout the offline build writes carries a
version, and a server refuses an artifact of another one rather than misread
it. That refusal is deliberate: a wrong display is worse than a failure to
start. **Expect to rebuild your artifacts when you upgrade**, and plan the
upgrade as build then switch, not switch then build. The
[install page](../operate/install.md) has the order.

**The `crates.io` crates.** `rf2`, `concept-graph`, `concept-store`,
`designation-index`, `sct-ecl`, `fhir-terminology`, and the rest are
published so other Rust projects can use them, on their own `0.x`
line that moves with every change to their contents. They are pre-1.0 and
their APIs change. The server's HTTP API is the stable surface; the crates are
not it.

**Performance figures.** The [benchmarks](benchmarks.md) are records of what
one machine measured, not a floor the project holds you to. The bars the
project does claim live in `bench/bars.json` and CI fails when one is crossed.

**Anything the conformance page marks as failing.** A suite case that does not
pass is a known gap with an issue, and fixing it changes behaviour. That is a
fix, and the promise does not protect a wrong answer.

## How a breaking change would arrive

Inside the 0.1 line, a change to a promised surface happens only to follow the
specification or to fix a wrong answer. It arrives with the issue that
adjudicated it, the spec citation, and a note in the changelog saying what
moved. If a promised surface has to change for any other reason, that is 0.2.
