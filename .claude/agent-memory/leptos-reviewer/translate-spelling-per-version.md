---
name: translate-spelling-per-version
description: the $translate parameter and match-part names per FHIR version, read off the vendored OperationDefinitions, plus which names come from the tx-ecosystem overlay rather than the base definition
csr: still-applies
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Read first-hand on 2026-09-07 from the `ConceptMap-translate`
`OperationDefinition` of each core package (`hl7.fhir.{r4,r4b,r5,r6}.core`),
which those packages left this repository with (#300); the published operation
page is <https://hl7.org/fhir/R4B/conceptmap-operation-translate.html>. Use
this to check a viewer spelling table without re-deriving it, but re-read the
definition before calling a mismatch a defect.

**In (the request).**

| version | code | system | system version | target system |
|---|---|---|---|---|
| R4, R4B | `code` | `system` | `version` | `targetsystem` |
| R5 | `sourceCode` | `system` | `version` | `targetSystem` |
| R6 ballot5 | `sourceCode` | `sourceSystem` | `sourceVersion` | `targetSystem` |

`url`, `conceptMap`, `conceptMapVersion`, and `dependency` are the same in all
four. R4/R4B alone declare `reverse`; the scopes are `source`/`target` in
R4/R4B and `sourceScope`/`targetScope` in R5/R6. Every `code`/`sourceCode`
documentation string says "If a code is provided, a system must be provided",
which is what makes a "do not send a half-named concept" gate spec-grounded.

**Out (`match.part`).** R4/R4B: `equivalence`, `concept`, `product`
(parts `element` + `concept`), `source`. R5/R6: `relationship`, `concept`,
`property` (parts `uri` + `value`), `product` and `dependsOn` (parts
`attribute` + `value`), `originMap`. `ConceptMapEquivalence` and
`ConceptMapRelationship` are different code sets, so a viewer must label which
element answered and never render one as the other.

**Not in any of the four base definitions:** `match.noMap`,
`match.sourceConcept`, `match.sourceComment`, `match.targetComment`, and the
top-level `used-conceptmap`. Those reach the wire through the terminology
ecosystem overlay the generator applies, emitted into the `fhir-types` R6
`concept_map_translate` operation, so a viewer reading them is correct and a
reviewer checking only the base definition will wrongly call them invented.
