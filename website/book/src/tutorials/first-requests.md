<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Your first requests

**Audience:** integrators who are new to FerroTERM. In about fifteen minutes
you load a small code system of your own, start the server, and send each of
the five FHIR terminology operations: `$lookup`, `$validate-code`,
`$subsumes`, `$expand` and `$translate`.

You need no licensed content. The code system, value set and concept map on
this page are invented for it, and the units of measure come with the
binary. A test in the repository writes every file on this page, starts the
server from its configuration and sends every request, so what you read here
is what the server answers.

<!-- toc -->

## 1. Get the server

Download the release tarball for your platform and unpack it, as
[Install and run](../operate/install.md#run-the-binary) shows. It holds the
`ferroterm` binary this page runs. Then make an empty working directory with
a `content` directory inside it, and run every command below from the
working directory:

```console
$ mkdir -p first-requests/content && cd first-requests
```

## 2. Write a code system

A code system is a list of codes with their meaning. This one names the
reasons a patient visits a clinic, in a hierarchy of two levels: a cough and
a wheeze are respiratory complaints, a sprain is an injury. Save it as
`content/CodeSystem-visit-reason.json`:

<!-- book-file: content/CodeSystem-visit-reason.json -->
```json
{
  "resourceType": "CodeSystem",
  "id": "visit-reason",
  "url": "https://example.org/fhir/CodeSystem/visit-reason",
  "version": "1.0.0",
  "name": "VisitReason",
  "title": "Visit reasons",
  "status": "active",
  "content": "complete",
  "hierarchyMeaning": "is-a",
  "concept": [
    {"code": "respiratory", "display": "Respiratory complaint", "concept": [
      {"code": "cough", "display": "Cough"},
      {"code": "wheeze", "display": "Wheeze"}
    ]},
    {"code": "injury", "display": "Injury", "concept": [
      {"code": "sprain", "display": "Sprain"}
    ]}
  ]
}
```

`hierarchyMeaning` says that a nested concept is a kind of its parent, which
is what `$subsumes` and an `is-a` filter read
([CodeSystem.hierarchyMeaning](https://hl7.org/fhir/R4/codesystem-definitions.html#CodeSystem.hierarchyMeaning)).

A second code system holds the groups a report counts visits in. Save it as
`content/CodeSystem-report-group.json`:

<!-- book-file: content/CodeSystem-report-group.json -->
```json
{
  "resourceType": "CodeSystem",
  "id": "report-group",
  "url": "https://example.org/fhir/CodeSystem/report-group",
  "version": "1.0.0",
  "name": "ReportGroup",
  "title": "Report groups",
  "status": "active",
  "content": "complete",
  "concept": [
    {"code": "resp", "display": "Respiratory"},
    {"code": "trauma", "display": "Trauma"}
  ]
}
```

## 3. Write a value set and a concept map

A value set selects codes. This one selects the respiratory complaint and
everything under it. Save it as `content/ValueSet-respiratory.json`:

<!-- book-file: content/ValueSet-respiratory.json -->
```json
{
  "resourceType": "ValueSet",
  "id": "respiratory",
  "url": "https://example.org/fhir/ValueSet/respiratory",
  "version": "1.0.0",
  "name": "Respiratory",
  "status": "active",
  "compose": {"include": [{
    "system": "https://example.org/fhir/CodeSystem/visit-reason",
    "filter": [{"property": "concept", "op": "is-a", "value": "respiratory"}]
  }]}
}
```

A concept map relates the codes of one system to another. This one puts each
visit reason in its report group. Save it as
`content/ConceptMap-visit-to-report.json`:

<!-- book-file: content/ConceptMap-visit-to-report.json -->
```json
{
  "resourceType": "ConceptMap",
  "id": "visit-to-report",
  "url": "https://example.org/fhir/ConceptMap/visit-to-report",
  "version": "1.0.0",
  "name": "VisitToReport",
  "status": "active",
  "group": [{
    "source": "https://example.org/fhir/CodeSystem/visit-reason",
    "target": "https://example.org/fhir/CodeSystem/report-group",
    "element": [
      {"code": "cough", "target": [{"code": "resp", "equivalence": "wider"}]},
      {"code": "wheeze", "target": [{"code": "resp", "equivalence": "wider"}]},
      {"code": "sprain", "target": [{"code": "trauma", "equivalence": "wider"}]}
    ]
  }]
}
```

The server reads the resources of a directory in the FHIR version its
`package.json` names. Save this as `content/package.json`, so the four files
read as FHIR R4:

<!-- book-file: content/package.json -->
```json
{"name": "example.first-requests", "version": "1.0.0", "fhirVersions": ["4.0.1"]}
```

## 4. Configure and start the server

Save the configuration as `ferroterm.toml`. `code_systems` names the
directories of FHIR resources to serve:

<!-- book-file: ferroterm.toml -->
```toml
[server]
listen = "127.0.0.1:8080"

[terminology]
code_systems = ["content"]
```

Check it, then start the server:

```console
$ ferroterm config check --config ferroterm.toml
$ ferroterm --config ferroterm.toml
```

`config check` prints every key with its value and where it came from. The
server logs one `serving code system` line per code system, among them
`visit-reason`, and a `listening` line naming the four FHIR bases: `/r4`,
`/r4b`, `/r5` and `/r6`. Leave it running and open a second terminal.

## 5. Send the requests

Each request below is written as HTTP. Send it with `curl` against the
address the server listens on, for example:

```console
$ curl 'http://127.0.0.1:8080/r4/metadata?mode=terminology'
```

Every answer is FHIR JSON. The excerpts show the parts that matter; the
server sends more.

### See what the server serves

<!-- book-request: capabilities expect=200 holds=/resourceType=TerminologyCapabilities -->
```http
GET /r4/metadata?mode=terminology
```

The `TerminologyCapabilities` lists every code system the server holds, with
its versions, filters and properties: your two systems, and UCUM, BCP 47,
BCP 13 and ISO 3166-1, which the binary carries.

### Look up a code

<!-- book-request: lookup expect=200 holds=display=Cough holds=version=1.0.0 -->
```http
GET /r4/CodeSystem/$lookup?system=https://example.org/fhir/CodeSystem/visit-reason&code=cough
```

```json
{"resourceType": "Parameters", "parameter": [
  {"name": "name", "valueString": "VisitReason"},
  {"name": "version", "valueString": "1.0.0"},
  {"name": "display", "valueString": "Cough"}
]}
```

The answer also carries the code's designations and its properties, such as
`parent`. Without a `version`, the server answers from the greatest version
it holds and names it.

### Check a code and its display

<!-- book-request: validate-display expect=200 holds=result=false holds=display=Cough -->
```http
GET /r4/CodeSystem/$validate-code?url=https://example.org/fhir/CodeSystem/visit-reason&code=cough&display=Coughing
```

```json
{"resourceType": "Parameters", "parameter": [
  {"name": "result", "valueBoolean": false},
  {"name": "message", "valueString": "Wrong Display Name 'Coughing' for https://example.org/fhir/CodeSystem/visit-reason#cough. Valid display is 'Cough' (for the language(s) '--')"},
  {"name": "display", "valueString": "Cough"}
]}
```

The code exists, the display does not match, and the answer names the
display that does.

### Ask whether one code subsumes another

<!-- book-request: subsumes expect=200 holds=outcome=subsumes -->
```http
GET /r4/CodeSystem/$subsumes?system=https://example.org/fhir/CodeSystem/visit-reason&codeA=respiratory&codeB=cough
```

```json
{"resourceType": "Parameters", "parameter": [{"name": "outcome", "valueCode": "subsumes"}]}
```

A cough is a respiratory complaint, so `respiratory` subsumes `cough`.

### Expand the value set

<!-- book-request: expand expect=200 holds=/expansion/total=3 holds=/expansion/contains/0/code=respiratory holds=/expansion/contains/0/contains/1/code=wheeze -->
```http
GET /r4/ValueSet/$expand?url=https://example.org/fhir/ValueSet/respiratory
```

```json
{"resourceType": "ValueSet", "expansion": {
  "total": 3,
  "contains": [{"code": "respiratory", "display": "Respiratory complaint", "contains": [
    {"code": "cough", "display": "Cough"},
    {"code": "wheeze", "display": "Wheeze"}
  ]}]
}}
```

The value set takes one system's own hierarchy, so the expansion keeps it:
the two complaints sit under their parent, and `total` counts all three.

### Check a code against the value set, by POST

Every operation also takes a `Parameters` resource by `POST`:

<!-- book-request: validate-member expect=200 holds=result=false -->
```http
POST /r4/ValueSet/$validate-code
Content-Type: application/fhir+json

{"resourceType": "Parameters", "parameter": [
  {"name": "url", "valueUri": "https://example.org/fhir/ValueSet/respiratory"},
  {"name": "coding", "valueCoding": {"system": "https://example.org/fhir/CodeSystem/visit-reason", "code": "sprain"}}
]}
```

A sprain is a valid code of the system and is not in the value set, so
`result` is `false` and the `message` says why.

### Translate a code

<!-- book-request: translate expect=200 holds=result=true holds=/parameter/1/part/1/valueCoding/code=trauma -->
```http
GET /r4/ConceptMap/$translate?url=https://example.org/fhir/ConceptMap/visit-to-report&system=https://example.org/fhir/CodeSystem/visit-reason&code=sprain
```

```json
{"resourceType": "Parameters", "parameter": [
  {"name": "result", "valueBoolean": true},
  {"name": "match", "part": [
    {"name": "equivalence", "valueCode": "wider"},
    {"name": "concept", "valueCoding": {"system": "https://example.org/fhir/CodeSystem/report-group", "code": "trauma"}},
    {"name": "source", "valueCanonical": "https://example.org/fhir/ConceptMap/visit-to-report|1.0.0"}
  ]}
]}
```

Each match names the map it came from, with its version.

### Look up a unit, with nothing loaded

<!-- book-request: ucum expect=200 holds=display=mg/dL -->
```http
GET /r4/CodeSystem/$lookup?system=http://unitsofmeasure.org&code=mg/dL
```

UCUM is a grammar the binary carries, so any valid unit answers, with its
canonical form as a property.

### Read a refusal

<!-- book-request: unknown-code expect=400 holds=/resourceType=OperationOutcome holds=/issue/0/details/coding/0/code=invalid-code -->
```http
GET /r4/CodeSystem/$lookup?system=https://example.org/fhir/CodeSystem/visit-reason&code=fever
```

```json
{"resourceType": "OperationOutcome", "issue": [{
  "severity": "error",
  "code": "code-invalid",
  "details": {
    "coding": [{"system": "http://hl7.org/fhir/tools/CodeSystem/tx-issue-type", "code": "invalid-code"}],
    "text": "code `fever` is not in code system `https://example.org/fhir/CodeSystem/visit-reason` version `1.0.0`"
  }
}]}
```

Every refusal is an `OperationOutcome`. A terminology failure carries a
coding from the `tx-issue-type` code system, which a client can branch on.

## Where to go next

- [The FHIR terminology API](../integrate/fhir-api.md) lists every operation,
  its parameters and its answers.
- [Value sets, implicit value sets, and ECL](../integrate/ecl-value-sets.md)
  covers the value sets a code system defines by URL, and SNOMED CT's
  Expression Constraint Language.
- [Loading code systems](../operate/loading-snomed.md) builds an index from a
  release you are licensed for, such as SNOMED CT or LOINC.
- [How FerroTERM serves terminology](../explanation/how-ferroterm-serves.md)
  explains the model behind these requests.
