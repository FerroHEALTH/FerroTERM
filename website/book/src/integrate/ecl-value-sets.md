<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Value sets, implicit value sets, and ECL

A value set names a set of codes. FerroTERM answers value sets defined as FHIR
resources, brought with the request, or implied by a URL a code system
defines. This page covers what is answered today and what the ECL milestone
adds.

<!-- toc -->

## Implicit value sets today

Several code systems define value sets by URL convention, without a stored
resource, and their providers answer them:

| Code system | URL | Members |
|---|---|---|
| LOINC | `http://loinc.org/vs` | every code |
| LOINC | `http://loinc.org/vs/LL…` | the answers of the answer list |
| LOINC | `http://loinc.org/vs/LP…` | everything under the part in the multiaxial hierarchy |
| UCUM | `http://unitsofmeasure.org/vs` | every valid unit (validation only; it cannot be enumerated) |
| RxNorm | `http://www.nlm.nih.gov/research/umls/rxnorm/vs` | every RXCUI |
| ICD-11 | `<entity URI>/postcoordinationScale/<axis>` | the values a stem takes on that axis |

Pass the URL as `url` to `ValueSet/$expand` or `ValueSet/$validate-code`.

## The SNOMED CT implicit value sets

The FHIR SNOMED CT page defines value sets by URL over the SNOMED CT URI
(<https://hl7.org/fhir/R4B/snomedct.html>, "Implicit Value Sets"), and the
provider answers every form:

| URL | Members |
|---|---|
| `http://snomed.info/sct?fhir_vs` | every concept of the edition |
| `http://snomed.info/sct?fhir_vs=isa/[sctid]` | the concept and its descendants, from the transitive closure |
| `http://snomed.info/sct?fhir_vs=refset` | the concepts that are reference sets with concept members |
| `http://snomed.info/sct?fhir_vs=refset/[sctid]` | the active concept members of the reference set |
| `http://snomed.info/sct?fhir_vs=ecl/[expression]` | the concepts the expression constraint selects; the expression is URI-encoded |

The base may be the edition URI (`http://snomed.info/sct/11000146104?fhir_vs=…`)
or the edition and version URI, which pins the expansion to that served
version; another edition's URI is refused. An unknown SCTID or a concept that
is not a reference set is `vs-invalid` with the code named. The reference set
memberships come from every RF2 reference set whose members are concepts (the
simple, association, attribute value, and map reference sets); the language
reference sets are read as acceptabilities instead.

The same sets are reachable through a `compose`, inline or loaded, with the
filters the provider declares: `concept is-a [sctid]`, `concept descendent-of
[sctid]`, and `concept in [refset]` (reference set membership, as the page
defines `in` for SNOMED CT), plus the generic `is-not-a`, `generalizes`,
`not-in`, and `regex`:

```json
{
  "resourceType": "ValueSet",
  "status": "active",
  "compose": {
    "include": [{
      "system": "http://snomed.info/sct",
      "filter": [{ "property": "concept", "op": "in", "value": "31000147101" }]
    }]
  }
}
```

## ECL

An expression constraint names a set of concepts, and the evaluator answers it
as set algebra over the precomputed structures: `<< X` is the descendant
bitmap plus `X`, `^ X` the members of the reference set, a refinement such as
`< 404684003 : 363698007 = << 39057004` the sources of the attribute with a
value in the set (from an inverted index), a grouped refinement counts the
role groups that satisfy it, and `AND`, `OR`, and `MINUS` are bitmap
operations. The parser follows the official ANTLR grammar for ECL 2.2 rule
for rule and is tested against the published example corpus. Every operator
evaluates, as do the concept filters, the history supplements, and alternate
identifiers. Four filters are refused because the artifact does not carry what
they ask for: a member filter on inactive members, an acceptability other than
preferred or acceptable, and the description module and effective time
filters. Each is refused by name rather than answered wrongly.

ECL reaches the wire two ways: the implicit value set
`http://snomed.info/sct?fhir_vs=ecl/[expression]` (the expression URI-encoded,
on the system, edition, or version URI) and the `constraint` filter of a
`ValueSet.compose.include`:

```json
{ "property": "constraint", "op": "=", "value": "< 404684003 |Clinical finding| : 363698007 |Finding site| = << 39057004" }
```

Malformed ECL is an `OperationOutcome` with issue code `invalid` naming the
byte offset; an identifier the edition does not have is `code-invalid`; a
construct the artifact cannot answer (a description module or effective time
filter, a filter on inactive members) is `not-supported`.

## Post-coordinated expressions

A SNOMED CT expression in Compositional Grammar is a valid `code` and is
"subject to the same rules as precoordinated concepts"
(<https://hl7.org/fhir/R4B/snomedct.html>), so you send one wherever you send a
concept id:

```text
POST [base]/CodeSystem/$validate-code
{"resourceType":"Parameters","parameter":[
  {"name":"url","valueUri":"http://snomed.info/sct"},
  {"name":"code","valueCode":"83152002 |Oophorectomy| : 405815000 |Procedure device| = 122456005 |Laser device|"}]}
```

Send an expression by POST. It carries spaces, pipes, and braces, so a GET
query string has to percent-encode every one of them.

- `$validate-code` checks the syntax and that the version defines every concept
  the expression names. A syntax error answers `false` with the byte the parser
  stopped at; an unknown concept answers `false` naming it.
- `$lookup` answers the expression in one canonical order as `code`, and a
  `display` that is the expression with the edition's terms written in, since
  SNOMED International publishes no terms for expressions. `normalForm` and
  `normalFormTerse` answer the expression's necessary normal form, and the
  expression's own refinement comes back as the concept-model properties keyed
  by attribute concept id. An expression over an inactive concept validates
  with `inactive = true`, the way an inactive concept does.
- `$subsumes` takes an expression as `codeA`, as `codeB`, or as both.
- `$closure` relates an expression to the concepts a table already holds.

The `expressions` filter decides whether a *filtered* include admits one:
`{"property": "expressions", "op": "=", "value": "true"}`. A filtered include
that states `false`, or states no `expressions` filter at all, refuses an
expression; one that states `true` admits an expression whose focus concepts it
contains. An include that *enumerates* an expression in
`compose.include.concept` contains it whichever way either side spells it, and
carries no filter at all, since `vsd-3` forbids one beside enumerated concepts.
An expansion lists only the expressions a value set enumerates, and
`excludePostCoordinated=true` drops those; a filtered selection stays the
`valueset-unclosed` case.

Three boundaries are deliberate. No concept model (MRCM) check runs, so an
expression the concept model forbids still validates. No close-to-user
transformation runs, so an expression is compared as written. Subsumption is
decided over the edition's inferred view without a description logic
classifier: what the server answers `subsumes` or `subsumed-by` holds, and
what it answers `not-subsumed` is what that view does not state. SNOMED
International withdrew normal-form comparison as a subsumption test in the
July 2019 International Edition and directs implementers to a classifier
(<https://docs.snomed.org/snomed-international-documents/snomed-ct-glossary/n/normal-form>).

## Paging a large expansion

A broad expansion names a large set, so `$expand` pages with `count` and
`offset` and refuses an unpaged expansion beyond 1,000 members with
`too-costly`. `expansion.total` reports the full size, `expansion.offset` the
page, and the order is deterministic across calls.
