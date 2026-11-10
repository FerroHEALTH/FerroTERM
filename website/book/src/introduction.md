<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Introduction

FerroTERM is a pure-Rust FHIR terminology server. It serves SNOMED CT, LOINC,
ICD-10, ICD-11, RxNorm, UCUM, and any FHIR `CodeSystem` through the HL7 FHIR
terminology API from one binary, over an index built once per code system
release and read at startup. There is no JVM, no Elasticsearch, and no database
to run.

> [!NOTE]
> The name: Ferro for the Rust family FerroTERM shares with FerroEHR, TERM for
> terminology. The site is <https://ferroterm.eu>.

## What this book leaves to the family book

FerroTERM is one product of the FerroHEALTH family, and Cadasto B.V. is the
manufacturer of every product. This book documents FerroTERM alone. The
manufacturer, the shared security policy and support period, the Cyber
Resilience Act's manufacturer side, how the products fit together, and the
licensing and trademark terms are written once, in the
[FerroHEALTH book](https://ferrohealth.eu/docs/).

## Who each part of this book is for

- **[Evaluate](evaluate/what-ferroterm-is.md)** is for anyone deciding whether
  FerroTERM fits: what it serves, why it exists, the architecture at a glance,
  and how it compares to Snowstorm, Ontoserver, and Hermes.
- **[Integrate](integrate/fhir-api.md)** is for API consumers: the FHIR
  terminology operations, the implicit value sets, the served FHIR version,
  and worked request and response examples captured from a running server.
- **[Operate](operate/install.md)** is for operators: installing and running
  the server, configuration, building an index from each code system's
  release, hardware sizing, and verifying a release before you run it.
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
