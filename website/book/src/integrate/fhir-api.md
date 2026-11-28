<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The FHIR terminology API

**Audience:** integrators who call the terminology operations from a FHIR
client. [Your first requests](../tutorials/first-requests.md) walks through
them step by step.

FerroTERM speaks the HL7 FHIR terminology API. If your client already talks to
a FHIR terminology server, it talks to FerroTERM. It speaks FHIR JSON and
FHIR XML: `_format=xml` in the query or `Accept: application/fhir+xml`
selects XML for a response, `Content-Type` names the format of a request
body, and JSON is the default. `Accept` quality values count, and an `Accept`
that names neither format gets `406 Not Acceptable`. The base is `/r4b` for R4B,
`/r4` for R4, `/r5` for R5, and `/r6` for the R6 ballot (see
[FHIR versions](fhir-versions.md)).

Two tasks have pages of their own: [Writing resources](persisted-resources.md)
covers the resources clients store and their history, and [Batch requests and
closure tables](batch-and-closure.md) covers `batch` Bundles and `$closure`.
The extensions FerroTERM adds to `metadata` are in [FerroTERM's FHIR
extensions](../reference/fhir-extensions.md).

<!-- toc -->

## The operations

| Operation | Answers | How |
|---|---|---|
| `CodeSystem/$lookup` | a concept's display, designations, and properties | a point read in the store; `property` narrows, `displayLanguage` picks the language |
| `CodeSystem/$validate-code` | whether the code exists and the display is right | the store; the right display comes back when the given one is wrong; inactive codes validate with a message |
| `CodeSystem/$subsumes` | `subsumes`, `subsumed-by`, `equivalent`, or `not-subsumed` | a bitmap membership test in the closure; either side may be a SNOMED CT expression ([Post-coordinated expressions](ecl-value-sets.md#post-coordinated-expressions)) |
| `ValueSet/$expand` | the members of a value set, paged | the compose layer over the providers' filters and bitmaps |
| `ValueSet/$validate-code` | whether a code is a member, and its display | membership without enumerating the set |
| `ConceptMap/$translate` | the targets a code maps to, with the relationship | loaded, inline, or request-scoped maps |
| `$versions` | the FHIR version of the base (`4.3`) | |
| `$cache-control` | a cache of request-scoped resources | below |
| `metadata`, `metadata?mode=terminology` | the `CapabilityStatement` and the `TerminologyCapabilities` listing every served code system with its filters and properties | |

Every operation accepts `GET` with query parameters and `POST` with a
`Parameters` resource, at the type level (`ValueSet/$expand?url=…`) and at
the instance level (`CodeSystem/{id}/$validate-code`,
`ValueSet/{id}/$expand`, `ValueSet/{id}/$validate-code`,
`ConceptMap/{id}/$translate`). The instance form runs the operation on the
resource the URL names (<https://hl7.org/fhir/R4B/operations.html#request>), so
it needs no `url`, and works for a resource that carries none of its own. A
`url`, a version, or an inline resource that names another one is a `400`; a
canonical carrying the instance's own version after a `|` is the instance. The levels a version offers are the ones its `OperationDefinition`
declares, so `CodeSystem/{id}/$lookup` answers under `/r5` and `/r6` and is
not a route under `/r4` and `/r4b`
(<https://hl7.org/fhir/R5/codesystem-operation-lookup.html>). `$expand` returns the
expanded `ValueSet`; the others return `Parameters`. Every failure is an
`OperationOutcome` whose issue carries `severity`, `code`, and `details.text`,
never a bare 500. A terminology failure adds a `details.coding` from
`http://hl7.org/fhir/tools/CodeSystem/tx-issue-type` (`invalid-code`,
`not-found`, `vs-invalid`, `too-costly`, and the rest); a refusal at the wire
layer, such as an unreadable id or a failed precondition, carries the status
and the text without one. The parameter set of each operation is what the
served version's `OperationDefinition` declares, plus the parameters the HL7
terminology ecosystem requires of every server
(<https://hl7.org/fhir/uv/tx-ecosystem/requirements.html>); a parameter outside
that set is refused.

A `$validate-code` against a code system the server does not hold answers `200`
with `result = false` and one `x-caused-by-unknown-system` per missing system,
so a validator can tell its user which resources to load. The value is a
canonical, `url` on its own or `url|version` when the request pinned a version
the server does not serve. `CodeSystem/$validate-code` names the system the
request asked for. `ValueSet/$validate-code` names a system the value set
itself selects from; a system the request brings that the value set never
selects from is reported as `x-unknown-system` instead, because the value set
is not the cause. Neither name is declared by any `OperationDefinition`, so
both come from the ecosystem overlay and are marked as such in the
`CapabilityStatement`.

## Value sets

A value set reaches `$expand` and `ValueSet/$validate-code` in five ways:

- **Loaded**: a `ValueSet` JSON under `FERROTERM__TERMINOLOGY__CODE_SYSTEMS`, named by `url`
  (and `version`; without one the greatest version answers). Loaded value sets
  are readable at `GET ValueSet/{id}` and `GET ValueSet?url=…&version=…`, where
  `{id}` is the `id` the resource carries, or one the server mints when it
  carries none (see
  [Configuration](../operate/configuration.md#what-a-code-system-version-is-served-as)).
- **Persisted**: a `ValueSet` written through the REST API (see below).
- **Inline**: the `valueSet` parameter of a `POST`.
- **Request-scoped**: `tx-resource` parameters, see below.
- **Implicit**: a URL a code system defines, answered by its provider without
  a resource: LOINC's `http://loinc.org/vs`, `/vs/[answer list]`, and
  `/vs/[part]`; UCUM's `/vs` and `/vs/[unit]`; RxNorm's `/vs`; ICD-11's
  `<entity>/postcoordinationScale/<axis>`; SNOMED CT's `?fhir_vs`,
  `?fhir_vs=isa/[sctid]`, `?fhir_vs=refset`, and `?fhir_vs=refset/[sctid]`,
  on the system, edition, or version URI ([Value sets and ECL](ecl-value-sets.md)).

A `compose` may include whole systems, enumerated concepts, and filters. The
filters a system answers are the ones its provider declares in
`TerminologyCapabilities`, and they differ by system, so read the declaration
rather than assuming a common set. SNOMED CT declares `concept` with `is-a`,
`descendent-of`, and `in`, then `constraint` for an expression constraint and
`expressions` for post-coordination
([Post-coordinated expressions](ecl-value-sets.md#post-coordinated-expressions)).
The others declare their own: LOINC's
`parent` and `ancestor` and its table fields, RxNorm's `STY`, `SAB`, `TTY`,
`REL`, and `RELA`, UCUM's `canonical` and `property`, ICD-10's note kinds. `$expand`
honours `count` and `offset` (at most 1,000 members without `count`), `filter`
(word-prefix text search over designations), `activeOnly`, `excludeNested`,
`includeDesignations`, `displayLanguage`, `system-version`,
`check-system-version`, `force-system-version`, and `exclude-system`, and
echoes every effective parameter plus one `used-codesystem` per system version
the expansion drew on.

An expansion nests where the compose is the system's own hierarchy: one include
of one system, taken whole or narrowed by `is-a` or `descendent-of`, with
nothing enumerated and nothing excluded. Each root then carries its children
under `contains`
(<https://hl7.org/fhir/R4B/valueset-definitions.html#ValueSet.expansion.contains>),
`total` counts every member rather than the roots, and a page is the slice of
the pre-order walk. `excludeNested=true` flattens it, and a text `filter` over a
whole system is flat already, because its matches have no root to hang from.

## Request-scoped resources

A client can bring its own `CodeSystem`, `ValueSet`, and `ConceptMap`
resources for one request: each `tx-resource` parameter of a `POST` carries
one, and the operation sees them layered over the loaded resources (a resource
with the same `url` and `version` as a loaded one shadows it for that request).
A supplement in a `tx-resource` applies to the system it names. Any other
resource type is refused with `not-supported`. This is what the HL7 terminology
ecosystem runner sends; `tx-resource` is a declared parameter in R6 and an
ecosystem extension on R4B
(<https://build.fhir.org/ig/HL7/fhir-tx-ecosystem-ig/requirements.html>).

To avoid resending the same resources, start a cache:

```text
POST [base]/$cache-control?mode=start   (Parameters of tx-resource)
→ Parameters { cache-id }
```

Then name it with the `X-Cache-Id` header on later requests; their own
`tx-resource`s stack on top. `POST [base]/$cache-control?mode=end` with the
header (or a `cache-id` parameter) releases it. A cache unused for 30 minutes
expires, and an unknown id answers `404`.

## Code system identity and versions

`system` is the code system's canonical URI (`http://snomed.info/sct`,
`http://loinc.org`, `http://hl7.org/fhir/sid/icd-10-cm`, …). Each loaded
version is one `CodeSystem` instance; without a `version` the server resolves
its default and echoes the version it used in every response. For SNOMED CT
the version is the edition and version URI of the SNOMED CT URI standard
(`http://snomed.info/sct/11000146104/version/20260630`), never a bare date.

Each instance is readable at `GET CodeSystem/{id}` and findable with
`GET CodeSystem?url=…&version=…`, on every version prefix, alongside the
resources a client persisted. A system loaded as a `CodeSystem` resource is read
at the `id` that resource carries; a system behind an index is read at the id
[Configuration](../operate/configuration.md#what-a-code-system-version-is-served-as)
describes and the server prints at startup. A system the server holds behind an index answers with its
definition and `content = not-present`: the codes come from `$lookup`,
`$validate-code`, and `$expand`, never from the resource
(<https://hl7.org/fhir/R4B/codesystem-content-mode.html>). A system loaded as a
`CodeSystem` resource under `FERROTERM__TERMINOLOGY__CODE_SYSTEMS` answers with the `content`
it declared, so a `complete` one carries its concepts. A supplement is applied
to the system it supplements and is read and searched like any other
`CodeSystem` this server holds, with `content = supplement` and the
`supplements` canonical it declares
(<https://hl7.org/fhir/R4B/codesystem.html#supplements>).

## Languages

`displayLanguage` and the `Accept-Language` header both select the display
and, on `$expand`, the displays of the members; the parameter wins when both
are given. Either carries a language range list with quality values
(`en, en-AU; q=0.4`, `de,*`), and the first range the code system carries, by
quality then by position, is the display language; `*` is the system's own.
A language the code system does not carry falls back to the system's own
(English for LOINC and ICD-11, the classification's language for a ClaML
document, the default language for SNOMED CT), and the response states what
it used. Designations carry their BCP 47 language and their use.

## SNOMED CT implicit concept maps

`ConceptMap/$translate` answers `url=http://snomed.info/sct?fhir_cm=[sctid]`,
where the SCTID names a reference set of the loaded edition. The edition or
edition-version URI may take the place of the bare system URI; the map states
the served edition version either way
(<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Concept Maps").

Two kinds of reference set answer. An association reference set maps a SNOMED
concept to another SNOMED concept, and the FHIR SNOMED CT page fixes the
relationship each one asserts:

| Reference set | SCTID | Relationship |
|---|---|---|
| POSSIBLY EQUIVALENT TO | `900000000000523009` | `inexact` |
| REPLACED BY | `900000000000526001` | `equivalent` |
| SAME AS | `900000000000527005` | `equal` |
| ALTERNATIVE | `900000000000530003` | `inexact` |

R5 and R6 have no `equal` or `inexact` code, so those become `equivalent` and
`related-to` on those versions, as the generated relationship vocabulary maps
them.

A map reference set maps a SNOMED concept to a code of another system through
`mapTarget`. No RF2 file records which system that code belongs to, so the
reference set itself says which scheme it maps to, and the group names that
system:

| Reference set | SCTID | Target system |
|---|---|---|
| ICD-10 extended map | `447562003` | `http://hl7.org/fhir/sid/icd-10` |
| ICD-9-CM equivalence complex map | `447563008` | `http://hl7.org/fhir/sid/icd-9-cm` |
| ICD-O simple map | `446608001` | `http://terminology.hl7.org/CodeSystem/icd-o-3` |
| CTV3 simple map | `900000000000497000` | `http://terminology.hl7.org/CodeSystem/read-Codes` |
| ICD-10-CM complex map (US Edition) | `6011000124106` | `http://hl7.org/fhir/sid/icd-10-cm` |

R4B needs the group to name a target system whenever the targets are real
codes: it may be left out only when the target value set names a single system,
or when every target equivalence is `unmatched`
(<https://hl7.org/fhir/R4B/conceptmap-definitions.html#ConceptMap.group.target>).
A map reference set outside the table, such as a national extension's own map,
is answered with `not-supported` rather than a target `Coding` that carries a
code and no system. RF2 records no version of the target scheme, so the group
names the system alone.

The complex and extended map columns (`mapGroup`, `mapPriority`, `mapRule`,
`mapAdvice`, `correlationId`, `mapCategoryId`) travel as `product` parts of the
match. No specification says where those columns belong in a `$translate`
result; this placement is FerroTERM's own design.

A `$translate` that names no `url` and finds no stored map falls back to the
historical associations of an inactive concept: the `SAME AS` and `REPLACED BY`
reference sets name what stands in its place, and each match names the implicit
map it came from. No specification asks for this fallback; it is FerroTERM's
own design. An active concept has no successors.

A `?fhir_cm=` naming a reference set the edition does not hold, or one that
maps nothing, is a `not-found`; a base that is not the served edition, or an
SCTID that does not parse, is a `400`.

An unpublished edition, named under `http://snomed.info/xsct/` (the SNOMED CT
URI Standard), takes implicit URIs on its own edition and version URIs too.

### The LOINC Extension map (provisional)

An edition that carries the SNOMED CT LOINC Extension answers
`[edition]/version/[date]?fhir_cm=11010000107` both ways: from a concept to the
LOINC code its RF2 identifier row gives it, and from a LOINC code back to the
concept, each `equivalent`. `$lookup` returns the same code as the `loinc`
property, and `$expand` takes the filter `mapped:30051010000102` `exists`
`true` or `false`. The tx-ecosystem IG's test edition notes are the only
source of these three, pending review by SNOMED International, so their shape
is FerroTERM's own and may change.

A `$translate` that names no map finds the implicit maps of the loaded code
systems between the source system and `targetSystem`, which may carry
`|version`, as the ecosystem IG requires ("the server SHALL find the
ConceptMaps whose `sourceScope` and `targetScope` fit the request").

