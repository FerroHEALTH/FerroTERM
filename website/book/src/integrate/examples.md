<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Worked examples

The requests below were sent to v0.1.0 serving the public ICD-10-CM FY2026
release, the RxNorm prescribable subset, and the built-in UCUM; the responses
are what the server returned, trimmed only where marked. Substitute the code
system you load.

<!-- toc -->

## $lookup a concept

```http
GET /r4b/CodeSystem/$lookup?system=http://hl7.org/fhir/sid/icd-10-cm&code=E11.9&property=parent
```

```json
{
  "resourceType": "Parameters",
  "parameter": [
    { "name": "name", "valueString": "International Classification of Diseases, Tenth Revision, Clinical Modification" },
    { "name": "version", "valueString": "2026" },
    { "name": "display", "valueString": "Type 2 diabetes mellitus without complications" },
    { "name": "designation", "part": [
      { "name": "language", "valueCode": "en" },
      { "name": "use", "valueCoding": { "system": "http://hl7.org/fhir/sid/icd-10-cm", "code": "preferred" } },
      { "name": "value", "valueString": "Type 2 diabetes mellitus without complications" } ] },
    { "name": "property", "part": [
      { "name": "code", "valueCode": "parent" },
      { "name": "value", "valueCode": "E11" } ] }
  ]
}
```

Without `property` every property comes back: the class kind, the notes
(`excludes1`, `codeAlso`, …), `parent`, `child`, and `valid`.

## $subsumes two concepts

```http
GET /r4b/CodeSystem/$subsumes?system=http://hl7.org/fhir/sid/icd-10-cm&codeA=E11&codeB=E11.9
```

```json
{ "resourceType": "Parameters", "parameter": [ { "name": "outcome", "valueCode": "subsumes" } ] }
```

## $validate-code with a wrong display

```http
GET /r4b/CodeSystem/$validate-code?url=http://hl7.org/fhir/sid/icd-10-cm&code=E11.9&display=Diabetes
```

```json
{
  "resourceType": "Parameters",
  "parameter": [
    { "name": "result", "valueBoolean": false },
    { "name": "message", "valueString": "the display `Diabetes` is not a designation of `E11.9`" },
    { "name": "display", "valueString": "Type 2 diabetes mellitus without complications" }
  ]
}
```

## $expand an inline value set with a code system filter

The drugs containing aspirin (RXCUI 1191), through RxNorm's `has_ingredient`
filter, two at a time:

```http
POST /r4b/ValueSet/$expand
Content-Type: application/fhir+json
```

```json
{
  "resourceType": "Parameters",
  "parameter": [
    { "name": "valueSet", "resource": {
      "resourceType": "ValueSet", "status": "active",
      "compose": { "include": [ {
        "system": "http://www.nlm.nih.gov/research/umls/rxnorm",
        "filter": [ { "property": "has_ingredient", "op": "=", "value": "CUI:1191" } ] } ] } } },
    { "name": "count", "valueInteger": 2 }
  ]
}
```

The response is the expanded `ValueSet` (metadata trimmed):

```json
{
  "resourceType": "ValueSet",
  "expansion": {
    "total": 93,
    "parameter": [
      { "name": "count", "valueInteger": 2 },
      { "name": "used-codesystem", "valueUri": "http://www.nlm.nih.gov/research/umls/rxnorm|09082026" }
    ],
    "contains": [
      { "system": "http://www.nlm.nih.gov/research/umls/rxnorm", "code": "1153231", "display": "aspirin / caffeine / orphenadrine Oral Product" },
      { "system": "http://www.nlm.nih.gov/research/umls/rxnorm", "code": "1153232", "display": "aspirin / caffeine / orphenadrine Pill" }
    ]
  }
}
```

## $lookup a unit, with nothing loaded

```http
GET /r4b/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg/dL
```

```json
{
  "resourceType": "Parameters",
  "parameter": [
    { "name": "name", "valueString": "Unified Code for Units of Measure (UCUM)" },
    { "name": "version", "valueString": "2.2" },
    { "name": "display", "valueString": "mg/dL" },
    { "name": "designation", "part": [
      { "name": "language", "valueCode": "en" },
      { "name": "value", "valueString": "milligram per deciliter" } ] },
    { "name": "property", "part": [
      { "name": "code", "valueCode": "canonical" },
      { "name": "value", "valueCode": "m-3.g" } ] }
  ]
}
```

## An error

An ICD-10-CM code without its period is not a code (the FHIR ICD page
requires the period), so the server answers `400` with an `OperationOutcome`:

```http
GET /r4b/CodeSystem/$lookup?system=http://hl7.org/fhir/sid/icd-10-cm&code=E119
```

```json
{
  "resourceType": "OperationOutcome",
  "issue": [ {
    "severity": "error",
    "code": "not-found",
    "details": {
      "coding": [ { "system": "http://hl7.org/fhir/tools/CodeSystem/tx-issue-type", "code": "invalid-code" } ],
      "text": "code `E119` is not in code system `http://hl7.org/fhir/sid/icd-10-cm` version `2026`"
    }
  } ]
}
```

See [The FHIR terminology API](fhir-api.md) for the operation list and
[Value sets and ECL](ecl-value-sets.md) for the implicit value set URLs.
