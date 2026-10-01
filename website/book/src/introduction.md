# Introduction

FerroTERM is a pure-Rust FHIR terminology server. It serves SNOMED CT, LOINC,
ICD-10, ICD-11, RxNorm, UCUM, and any FHIR `CodeSystem` through the HL7 FHIR
terminology API from one binary, over an index built once per code system
release and read at startup. There is no JVM, no Elasticsearch, and no database
to run.

> [!NOTE]
> The name: Ferro for the Rust family FerroTERM shares with FerroEHR, TERM for
> terminology. The site is <https://ferroterm.eu>.

## Where the project is

v0.1.6 is the current release. It ships as Linux binaries for `x86_64` and
`aarch64` (glibc and musl), as a container image for `linux/amd64` and
`linux/arm64`, and as a Compose file, all built with signed provenance. The
server answers FHIR R4 under `/r4`, R4B under `/r4b`, R5 under `/r5`, and the
R6 ballot under `/r6`; the CI lane runs the HL7 terminology ecosystem suite
against the published versions on every change and holds a committed pass list
per version.

Hierarchical expansion, persisted client resources, `Bundle` batch, `$closure`,
and the SNOMED implicit concept maps are served. The persisted resources and the
closure tables need a deployment that names a database in `FERROTERM_RESOURCES`,
and `$closure` answers on `/r4`, `/r4b`, and `/r5`, since the R6 ballot defines
no such operation. Where a mature terminology server does more, such as
authoring SNOMED content and MRCM validation of post-coordinated expressions,
this book says so. Each open item is an issue with acceptance criteria on the
tracker, and the milestones are the roadmap.

## Who each part of this book is for

- **[Evaluate](evaluate/what-ferroterm-is.md)** is for anyone deciding whether
  FerroTERM fits: what it serves, why it exists, the architecture at a glance,
  and how it compares to Snowstorm, Ontoserver, and Hermes.
- **[Operate](operate/install.md)** is for operators: installing and running
  the server, configuration, building an index from each code system's
  release, hardware sizing, and verifying a release before you run it.
- **[Integrate](integrate/fhir-api.md)** is for API consumers: the FHIR
  terminology operations, the implicit value sets, the served FHIR version,
  and worked request and response examples captured from a running server.
- **[Contribute](contribute/build-and-test.md)** is for contributors: building
  and testing, the FHIR code generation model, and where the deeper design
  lives.

## Two things that are always true

You bring your own code system content, and the software is source-available
under the Business Source License 1.1: free to use outside production and in
non-commercial production, a commercial licence for other production use, and
Apache License 2.0 four years after each version. SNOMED CT is licensed by SNOMED International, LOINC by the
Regenstrief Institute, ICD by the WHO, and RxNorm by the NLM; this repository
and every build of FerroTERM ship none of their content. You load a release you
are licensed for, and the server serves it. See
[Loading code systems](operate/loading-snomed.md).
