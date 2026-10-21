<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FHIR versions

FerroTERM answers FHIR R4 under `/r4`, R4B under `/r4b`, R5 under `/r5`, and
the R6 ballot (6.0.0-ballot5) under `/r6`, marked ballot-tracking.

<!-- toc -->

## Why one server can serve four versions

HL7 publishes the whole type system and every operation as machine-readable
resources, in versioned packages. FerroTERM vendors and pins those packages and
generates per-version Rust modules from them, so each version's operation
surface is correct by construction. An operation parameter that R5 adds appears
in the R5 module and is absent from R4 and R4B, because the generator emits
what each package declares. The engine beneath is version-neutral; a version's
endpoint maps the engine's answers into that version's shapes. R4 and R4B
declare the same terminology parameters and elements, so one set of macros
instantiates both endpoints from their own generated modules and they cannot
drift apart.

## Version status

| Version | Package | Status |
|---|---|---|
| R4 | `hl7.fhir.r4.core` 4.0.1 | served under `/r4` |
| R4B | `hl7.fhir.r4b.core` 4.3.0 | served under `/r4b` |
| R5 | `hl7.fhir.r5.core` 5.0.0 | served under `/r5` |
| R6 | `hl7.fhir.r6.core` 6.0.0-ballot5 | served under `/r6`, marked ballot-tracking |

R4B is the current stable release of the R4 line and a near-superset of R4;
both endpoints serve the same engine, and a `$cache-control` cache started on
one serves the other. The R5 shapes matter most to validators and IG tooling
(`$validate-code` `issues` and the validated `code`, `system`, and `version`;
`$lookup` `definition`; `$expand` `property`), and `/r5` emits them; the R4
and R4B endpoints emit only what their own definitions declare.

## What this means for you

Send R4 requests to `/r4`, R4B requests to `/r4b`, R5 requests to `/r5`, and
R6 ballot requests to `/r6`. `GET {base}/$versions` names the version of the
base (`4.0`, `4.3`, `5.0`, or `6.0`), `GET {base}/metadata` is that version's
`CapabilityStatement`, and a parameter the version does not declare is refused
with an `OperationOutcome` rather than absorbed.

The `/r6` endpoint answers in the ballot's shapes. Its capability statement
says which ballot it serves and that every behaviour grounded only in the
ballot is re-verified against the published specification when it appears. A
ballot parameter the server does not implement yet (`manifest`,
`filterProperty`) is refused as `not-supported`, never absorbed.
