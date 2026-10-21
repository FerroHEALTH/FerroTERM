<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# The FHIR terminology API

FerroTERM speaks the HL7 FHIR terminology API. If your client already talks to
a FHIR terminology server, it talks to FerroTERM. It speaks FHIR JSON and
FHIR XML: `_format=xml` in the query or `Accept: application/fhir+xml`
selects XML for a response, `Content-Type` names the format of a request
body, and JSON is the default. `Accept` quality values count, and an `Accept`
that names neither format gets `406 Not Acceptable`. The base is `/r4b` for R4B,
`/r4` for R4, `/r5` for R5, and `/r6` for the R6 ballot (see
[FHIR versions](fhir-versions.md)); the
[Worked examples](examples.md) page shows requests and the responses a running
server gave.

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

- **Loaded**: a `ValueSet` JSON under `FERROTERM_CODESYSTEMS`, named by `url`
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
`CodeSystem` resource under `FERROTERM_CODESYSTEMS` answers with the `content`
it declared, so a `complete` one carries its concepts. A supplement is applied
to the system it supplements and is read and searched like any other
`CodeSystem` this server holds, with `content = supplement` and the
`supplements` canonical it declares
(<https://hl7.org/fhir/R4B/codesystem.html#supplements>).

## Which artifact a code system came from

`metadata?mode=terminology` names the artifact behind each served version, so a
client can tell which built index answered without asking the operator. No FHIR
specification records that fact, so FerroTERM defines an extension for it, the
form FHIR provides for information outside a resource's basic definition
(<https://hl7.org/fhir/R4B/extensibility.html>). It sits on
`TerminologyCapabilities.codeSystem.version`, which is where the fact belongs:
one artifact holds one version, and two SNOMED CT editions share one
`codeSystem` entry. Every served FHIR version carries it, because R4, R4B, R5,
and the R6 ballot all define that element as a `BackboneElement`.

The canonical is
`https://ferroterm.eu/fhir/StructureDefinition/terminology-artifact`, with two
`string` sub-extensions:

- `name`: the artifact directory's own name, without the directories above it.
- `release`: the release identifier the offline build wrote into the artifact's
  manifest. For SNOMED CT this is the release date, for the others the release
  version.

```json
{
  "url": "https://ferroterm.eu/fhir/StructureDefinition/terminology-artifact",
  "extension": [
    { "url": "name", "valueString": "int" },
    { "url": "release", "valueString": "20260901" }
  ]
}
```

The definition is the `StructureDefinition` at
<https://ferroterm.eu/fhir/StructureDefinition/terminology-artifact>, and the
JSON is
[`terminology-artifact.json`](https://ferroterm.eu/fhir/StructureDefinition/terminology-artifact.json).

A version that came from no artifact carries no extension: the registries the
binary holds (UCUM, BCP 13, BCP 47, ISO 3166) and any `CodeSystem` resource a
deployment loaded or persisted. Nothing about the operator's filesystem and
nothing from the code system's content reaches the wire here.

## Which implicit value sets a code system resolves

`metadata?mode=terminology` also lists, per served version, the implicit value
set forms that version resolves. No element of `TerminologyCapabilities` says
which implicit value sets a server answers
(<https://hl7.org/fhir/R5/terminologycapabilities.html>), so FerroTERM defines a
second extension on `TerminologyCapabilities.codeSystem.version`, repeated once
per form, on R4, R4B, R5, and R6.

The canonical is
`https://ferroterm.eu/fhir/StructureDefinition/implicit-value-set`, with two
sub-extensions:

- `pattern` (`valueString`): the value set URL template, spelled as the code
  system's page spells it, with each placeholder in square brackets.
- `argument` (`valueCode`): what the placeholders take. `none` means the
  pattern is the URL as it stands, `code` means the one placeholder takes a code
  of the system, and `expression` means the placeholders take free text such as
  an ECL expression or a UCUM unit.

```json
{
  "url": "https://ferroterm.eu/fhir/StructureDefinition/implicit-value-set",
  "extension": [
    { "url": "pattern", "valueString": "http://snomed.info/sct?fhir_vs=isa/[sctid]" },
    { "url": "argument", "valueCode": "code" }
  ]
}
```

The definition is the `StructureDefinition` at
<https://ferroterm.eu/fhir/StructureDefinition/implicit-value-set>, and the
JSON is
[`implicit-value-set.json`](https://ferroterm.eu/fhir/StructureDefinition/implicit-value-set.json).

The forms declared today:

| System | Patterns |
|---|---|
| SNOMED CT | `?fhir_vs`, `?fhir_vs=isa/[sctid]`, `?fhir_vs=refset`, `?fhir_vs=refset/[sctid]`, `?fhir_vs=ecl/[ecl]` on `http://snomed.info/sct` |
| LOINC | `http://loinc.org/vs`, `/vs/[id]` (an answer list), `/vs/[partcode]` |
| RxNorm | `http://www.nlm.nih.gov/research/umls/rxnorm/vs` |
| UCUM | `http://unitsofmeasure.org/vs`, `/vs/[expression]` |
| ICD-11 | `[entity]/postcoordinationScale/[axis]`, on a linearization |

A version that resolves no implicit value set carries no such extension: the
ClaML classifications, BCP 13, BCP 47, ISO 3166, and every `CodeSystem`
resource. A supplemented system declares the forms of the system it
supplements. A declared form can still answer an error for one argument, for
example `?fhir_vs=refset` on an edition that defines no reference sets.

## Which levels an operation answers at

A `CapabilityStatement` records that a server answers an operation and never
at which level; `OperationDefinition.system`, `.type` and `.instance` carry
that (<https://hl7.org/fhir/R4B/operationdefinition-definitions.html#OperationDefinition.instance>).
`metadata` states the levels the version's own definition declares as an
extension on `CapabilityStatement.rest.resource.operation`, repeated once per
level, on R4, R4B, R5 and R6. The canonical is
`https://ferroterm.eu/fhir/StructureDefinition/operation-level`, with a
`valueCode` of `system`, `type` or `instance`:

```json
{
  "name": "validate-code",
  "definition": "http://hl7.org/fhir/OperationDefinition/CodeSystem-validate-code",
  "extension": [
    { "url": "https://ferroterm.eu/fhir/StructureDefinition/operation-level", "valueCode": "type" },
    { "url": "https://ferroterm.eu/fhir/StructureDefinition/operation-level", "valueCode": "instance" }
  ]
}
```

The definition is the `StructureDefinition` at
<https://ferroterm.eu/fhir/StructureDefinition/operation-level>, and the JSON is
[`operation-level.json`](https://ferroterm.eu/fhir/StructureDefinition/operation-level.json).

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

## Persisted resources

A deployment that names a database in `FERROTERM_RESOURCES` accepts
`CodeSystem`, `ValueSet`, and `ConceptMap` resources over the REST API and
keeps them across restarts. The interactions are the ones the FHIR RESTful API
defines (<https://hl7.org/fhir/R4B/http.html>), under every version prefix:

| Request | Answer |
|---|---|
| `POST {type}` | `201 Created` with `Location`, `ETag`, and `Last-Modified`; the server assigns the id; `409 Conflict` when another resource carries the `url` and `version` |
| `PUT {type}/{id}` | `200 OK` when the id existed, `201 Created` when it is new, `409 Conflict` when the id is one a loaded resource is read at or another resource carries the `url` and `version` |
| `GET {type}/{id}` | `200 OK` with `ETag` and `Last-Modified`, `404` for an unknown id or one this version has no representation of, `410 Gone` for a deleted one |
| `GET {type}/{id}/_history/{versionId}` | `200 OK` with that version of the resource, `410 Gone` for the version a delete made |
| `GET {type}/{id}/_history` | a `history` `Bundle` of every version, newest first; empty for a loaded resource |
| `GET {type}/_history` | a `history` `Bundle` of every version of every persisted resource of the type |
| `GET {type}?url=…&version=…` | a `searchset` `Bundle`, `entry.fullUrl` absolute |
| `DELETE {type}/{id}` | `204 No Content`, and deleting again has no effect and is not an error |

`meta.versionId` starts at `1` and rises with every write; the `ETag` is its
weak form (`W/"2"`). Send `If-Match` with that value on a `PUT` or a `DELETE`
to make the write conditional; a value that names another version is a
`412 Precondition Failed`. A delete counts as a version too, so a resource
written again after a delete carries on from the version after it.

A history entry carries the version in `resource`, the interaction that made
it in `request` (`POST`, `PUT` or `DELETE` with its relative URL), and its
status, `etag` and `lastModified` in `response`; the delete has no resource
(<https://hl7.org/fhir/R4B/http.html#history>). `_since` keeps the versions
written at or after an instant. `_count`, `_at` and `_list` answer `400` with
a `not-supported` issue. A version written on a base whose release this base
cannot read keeps its `response`, with the reason in `response.outcome`, and
has no `request`, because an entry that names `PUT` or `POST` must carry the
resource.

A `url` and `version` identify one resource
(<https://hl7.org/fhir/R4B/resource.html#canonical>). A write whose pair
another resource of the same type already carries, one written through the API
or one the deployment loaded, answers `409 Conflict` with an issue of code
`duplicate` naming that resource's id. Update the resource that holds the pair,
or write the same `url` under a new `version`.

A read and a search both take `_elements` and `_summary`
(<https://hl7.org/fhir/R4B/search.html#summary>, <https://hl7.org/fhir/R4B/http.html#read>).
`_elements=url,title` returns those elements plus `resourceType`, `id` and
`meta`. `_summary=true` returns `id`, `meta` and the top-level elements the
version's definition marks as summary elements (for a `CodeSystem`, `url`,
`version`, `name`, `status` and the rest of its metadata, without `concept`).
`_summary=text` returns `text`, `id`, `meta` and the top-level
elements the version's definition makes mandatory, `_summary=data` drops
`text`, `_summary=false` is the whole resource, and on a search
`_summary=count` returns `total` and no entries. A resource that lost an
element carries the `SUBSETTED` tag, so do not write it back. A value outside
those five is a `400`.

Each `entry.fullUrl` is the absolute URL of the resource
(<https://hl7.org/fhir/R4B/bundle.html#bundle-unique>): the base URL of
`FERROTERM_BASE_URL` plus the version's own root, or, where a deployment
declares no base URL, the authority the request names. Declare the base URL
behind TLS or a path prefix, because a request carries no scheme. The bundle
also carries its `self` link, the URL the search was made at, which R5 and the
R6 ballot require of every searchset (`bdl-18`,
<https://hl7.org/fhir/R5/bundle.html>).

A resource is stored as the JSON it arrived as, with the FHIR version of the
endpoint that received it, and a read renders it through the reading version's
own codec. A written resource is never converted between FHIR releases. R4B states a
target's direction with `concept-map-equivalence` and R5 with
`concept-map-relationship`; `equal`, `subsumes`, `specializes`, `inexact` and
`unmatched` have no code in the R5 set
(<https://hl7.org/fhir/R4B/valueset-concept-map-equivalence.html>,
<https://hl7.org/fhir/R5/valueset-concept-map-relationship.html>), and HL7's
R4-to-R5 transforms live in a cross-version implementation guide that is
informative and under development
(<https://hl7.org/fhir/extensions/conversions-ConceptMap.html>), so no core
specification states what a conversion should produce.

A search on a version that has no representation of a stored resource leaves
that resource out of its matches and adds one entry carrying an
`OperationOutcome` with `search.mode` of `outcome`, `severity` of `warning`,
`code` of `not-supported`, and text naming the resource and the version it was
written as (<https://hl7.org/fhir/R4B/http.html#search>); `total` counts the
matches, and the rest of the searchset answers as usual. Reading that resource
by id on that version is a `404`: serving one release per base is the strategy
the specification describes as one where "the same record has a different
identity depending on the version of FHIR in use"
(<https://hl7.org/fhir/R5/versioning.html>), and a read answers `200`, `404` or
`410` and nothing else (<https://hl7.org/fhir/R4B/http.html#read>). The
`OperationOutcome` names the release the resource does read as, so read it
there.

Every operation sees a persisted resource exactly as it sees one loaded from
`FERROTERM_CODESYSTEMS`, on every served version.

A deployment that names no database refuses every write with a `422` and
declares no write interaction in its capability statement.

## Primitive lexical forms on local content

A resource you write is decoded by the generated FHIR model of the version
you send it to, and since `fhir-types` 0.1.105 that decode checks every
primitive against the regex its FHIR package publishes for the type
(<https://hl7.org/fhir/R5/datatypes.html#primitive>). A `date` of
`yesterday`, a `valueDateTime` of `2001-6`, a `url` with a space, or a
`status` with a leading space answers `400` with an `OperationOutcome` whose
`issue.code` is `invalid` and whose `issue.expression` names the element
(`CodeSystem.date`). The versions differ where their packages differ: R4,
R4B, and the R6 ballot require a timezone offset once a `dateTime` carries a
time, R5 leaves it optional. Nothing is stored that the version's own codec
refuses, so a later read never meets a value it cannot parse.

## Concept status on local content

A code you author has a lifecycle, and FHIR keeps it in two places. The
resource's own standing is `CodeSystem.status`, so a system you are still
drafting is a `draft` CodeSystem; no version defines a per-concept `draft`. A
single concept states its standing in the standard concept properties
(<https://hl7.org/fhir/R5/codesystem-concept-properties.html>):

| Property | Type | What the server reads |
|---|---|---|
| `inactive` | boolean | `true` makes the concept inactive |
| `status` | code | `retired` makes the concept inactive; `active`, `experimental`, `deprecated`, and any other value leave it active |
| `retirementDate` | dateTime | a date the request is behind makes the concept inactive |
| `deprecated`, `deprecationDate` | dateTime | a date the request is behind marks the concept deprecated: it stays active and `$validate-code` adds the `DEPRECATED_CONCEPT_FOUND` warning, the same note the standards-status extension earns; `status = deprecated` does the same |
| `deprecated`, `deprecationDate` | dateTime | returned as written, and the concept stays active |

R4 and R4B define `inactive` and `deprecated`, and R5 adds the rest. A
persisted resource is stored as it arrived, so the R5 properties are data on
every version prefix and are read the same way on R4, R4B, R5, and R6.

Deprecated is not inactive: the specification says concepts "that are
deprecated but not inactive can still be used, but their use is discouraged".
Mark a concept `inactive`, or give it a `status` of `retired`, when it must
stop being chosen.

Three points no FHIR specification settles, which the server decides for
itself. Any marker that says inactive wins, so `inactive = false` beside a
`status` of `retired` reads as inactive. A `retirementDate` is compared against
the time of the request, and a value that is not a readable `dateTime` reads as
a retirement already in force, so a typo never leaves a retired code active. A
`retirementDate` of `2026` or `2026-03` takes effect at the start of the period
it names.

Retire a concept rather than delete it. An inactive concept keeps answering:
`$expand` with `activeOnly=true` leaves it out, `activeOnly=false` lists it
with `contains.inactive = true`, `$validate-code` answers `result = true` with
`inactive = true` and a warning, and `$lookup` returns the properties as
written. Removing the concept from the resource instead makes every record that
already carries the code fail validation. The `status` output of
`$validate-code` reports a status only where the resource stated one, so a
concept retired by a date alone answers `inactive` with no `status`.

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

## Batch

`POST [base]` with a `Bundle` of type `batch` runs every entry and answers a
`batch-response` with one entry per request, in the order they were sent
(<https://hl7.org/fhir/R4B/http.html#transaction>). The entries are
independent: one that fails answers the `OperationOutcome` that the same
request would have answered on its own, the rest still answer, and the batch
itself answers `200`.

Each entry names its request the way it would have been sent on its own:

```json
{"resourceType": "Bundle", "type": "batch", "entry": [
  {"request": {"method": "GET", "url": "CodeSystem/$lookup?system=http://loinc.org&code=1963-8"}},
  {"request": {"method": "POST", "url": "ValueSet/$validate-code"},
   "resource": {"resourceType": "Parameters", "parameter": [
     {"name": "url", "valueUri": "http://loinc.org/vs"},
     {"name": "coding", "valueCoding": {"system": "http://loinc.org", "code": "1963-8"}}
   ]}}
]}
```

A `GET` entry carries the operation's inputs in the query of `request.url`; a
`POST` entry carries them in a `Parameters` resource on the entry. Every
terminology operation this server answers is reachable, at the type level and
at the instance level, and `entry.fullUrl` comes back on the response entry so
a client can pair the two. `response.status` is the code and its reason, such
as `200 OK` or `404 Not Found`.

A `transaction` Bundle is refused with `not-supported`: a transaction succeeds
or fails as one unit across its entries, and this server does not process one.

A failed entry carries its `OperationOutcome` as the entry's `resource`, the
body the same request would have answered alone. `entry.response.outcome` is
reserved by the specification for hints and warnings and is "not used for error
responses in batch/transaction", and no version names another slot for the
error, so this placement is FerroTERM's reading of an ambiguous rule.

## Closure tables

`POST [base]/$closure` maintains a named transitive closure table for a client
(<https://hl7.org/fhir/R4B/terminology-service.html>, "Maintaining a Closure
Table"). The client registers concepts as it meets them and the server answers
with the subsumption relationships that hold between them, as a `ConceptMap`
delta. Any code system with subsumption maintains a closure, not only SNOMED CT.

| Request | Answer |
|---|---|
| `name` alone | the table, created or emptied, as an empty `ConceptMap` at version `0` |
| `name` and one or more `concept` | a `ConceptMap` of the relationships the client did not have, at a new version |
| `name` and `version` | everything the server sent after that version, at the server's latest version |

A `version` of `0` resynchronises the whole table. Pass a `concept` or a
`version`, never both. The version is the server's own value: treat it as
opaque and hand it back unchanged.

An entry's `equivalence` is read from target to source, so `subsumes` means the
target subsumes the source. R4 and R4B use `equal`, `subsumes`, and
`specializes`; R5 states the same relationships as `equivalent`,
`source-is-narrower-than-target`, and `source-is-broader-than-target`. A concept
is never related to itself, and a client assumes that relationship. Two concepts
that subsume neither way get no entry.

Naming a table the server has not been asked to create is a `404`; an add never
creates one. Registering a concept whose code system changed under the table
answers `422` with `closure "[name]" must be reinitialized`, and the client's
move is to initialise and replay its codes.

The R6 ballot ships no `ConceptMap-closure` definition, so `/r6` offers no
`$closure` and its capability statement declares none. The tables live in the
database `FERROTERM_RESOURCES` names, so they outlive a restart.
