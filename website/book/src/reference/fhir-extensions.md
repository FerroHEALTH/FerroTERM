<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FerroTERM's FHIR extensions

**Audience:** integrators who read `metadata`. FerroTERM defines three
extensions for facts no FHIR element records; each section names the element
an extension sits on, its canonical, and its `StructureDefinition`.

<!-- toc -->

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
