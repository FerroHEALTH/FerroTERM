<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Batch requests and closure tables

**Audience:** integrators who send several terminology requests at once,
or keep a transitive closure table in step with the codes they meet.

<!-- toc -->

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
database `FERROTERM__TERMINOLOGY__RESOURCES` names, so they outlive a restart.
