<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Writing resources

**Audience:** integrators and terminologists who write their own
`CodeSystem`, `ValueSet`, `ConceptMap`, `NamingSystem` and `Provenance`
resources to the server, and read them back with their history.

<!-- toc -->

## Persisted resources

A deployment that names a database in `FERROTERM__TERMINOLOGY__RESOURCES` accepts
`CodeSystem`, `ValueSet`, `ConceptMap`, `NamingSystem` and `Provenance` resources over the REST API and
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
`FERROTERM__FHIR__BASE_URL` plus the version's own root, or, where a deployment
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
`FERROTERM__TERMINOLOGY__CODE_SYSTEMS`, on every served version.

A deployment that names no database refuses every write with a `422` and
declares no write interaction in its capability statement.

## The version and provenance of a mapping

The draft EEHRxF implementing act (Art 6) admits a national code only with a
validated mapping whose versions and provenance are recorded. Every `match` of
`ConceptMap/$translate` names the map it came from as a versioned canonical,
`url|version`, in `source` (R4 and R4B) or `originMap` (R5 and R6)
(<https://hl7.org/fhir/R4/conceptmap-operation-translate.html>).

Who validated that map, and when, is a `Provenance` whose `target.reference`
is the same canonical (<https://hl7.org/fhir/R4B/provenance.html>). The server
keeps the `Provenance` resources of every package directory under
`FERROTERM__TERMINOLOGY__CODE_SYSTEMS`, and `ferroterm-build --fhir-package`
keeps them on import; with a resource database, a client writes more. Read
them by the canonical:

```console
$ curl 'https://tx.example.org/r4b/Provenance?target=http://example.org/fhir/ConceptMap/national-to-mvc%7C10.1.0'
```

The answer is a `searchset` of every `Provenance` whose `target.reference` is
that string as written; `Provenance/{id}` reads one. A `Provenance` decodes in
the FHIR version it was loaded or written in, and a base whose version cannot
read it leaves it out, as it does for any persisted resource.

## Identifier and code system namespaces

A `NamingSystem` names an identifier or code system namespace and lists the
identifiers it goes by: a URI, an OID, or a local name such as the assigning
authority an HL7 v2 `CX.4` carries (<https://hl7.org/fhir/R4/namingsystem.html>).
The server keeps the `NamingSystem` resources of every package directory under
`FERROTERM__TERMINOLOGY__CODE_SYSTEMS`, and `ferroterm-build --fhir-package`
keeps them on import; with a resource database, a client writes more. Look a
namespace up by any of its identifiers:

```console
$ curl 'https://tx.example.org/r4/NamingSystem?kind=identifier&value=NATPAT'
```

The search answers `kind`, `value`, `type`, `id-type` and `status`
(<https://hl7.org/fhir/R4/namingsystem.html#search>), with `_elements` and
`_summary`. `value` matches any `uniqueId.value`: by default a value the
identifier starts with, ignoring case and accents, `value:exact` the identifier
as written, and `value:contains` a value anywhere in it
(<https://hl7.org/fhir/R4/search.html#string>). `kind`, `status` and `id-type`
take a code, or `system|code` under the code system their element is bound to;
`type` matches a coding of `NamingSystem.type`. Several values of one parameter,
separated by commas, match any of them, and several parameters must all match.
Any other parameter or modifier is a `400`. `NamingSystem/{id}` reads one.

## Primitive lexical forms on local content

A resource you write is decoded by the generated FHIR model of the version
you send it to, and that decode checks every
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
