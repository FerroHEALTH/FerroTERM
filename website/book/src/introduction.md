<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FerroTERM

FerroTERM is a FHIR terminology server for SNOMED CT, LOINC and other clinical
code systems. It answers the HL7 FHIR terminology operations on R4, R4B, R5
and the R6 ballot from one binary, over an index it builds once per code
system release and reads at startup. There is no JVM, no Elasticsearch and no
database to run. You bring the code system content you are licensed for;
FerroTERM ships none.

## Start here

| You are | You want to | Start with | Then |
|---|---|---|---|
| An integrator | Call the terminology operations from a FHIR client, a validator or an EHR | [Your first requests](tutorials/first-requests.md) | [The FHIR terminology API](integrate/fhir-api.md), [Value sets, implicit value sets, and ECL](integrate/ecl-value-sets.md), [Writing resources](integrate/persisted-resources.md) |
| An operator | Deploy, load content, configure, secure and upgrade the server | [Install and run](operate/install.md) | [Loading code systems](operate/loading-snomed.md), [Configuration](operate/configuration.md), [Behind a reverse proxy](operate/reverse-proxy.md), [Reloading and replacing content](operate/reloading.md) |
| A terminologist | Browse the content, compose value sets and keep them current with each release | [The console](operate/console.md) | [Syncing from a national terminology service](operate/sync.md), [Writing resources](integrate/persisted-resources.md) |
| A decision maker | Judge whether FerroTERM fits: what it serves, how it conforms, what it costs to run and to license | [How FerroTERM serves terminology](explanation/how-ferroterm-serves.md) | [Code systems served](evaluate/code-systems.md), [Conformance](evaluate/conformance.md), [Benchmarks](evaluate/benchmarks.md), [Licensing](evaluate/licensing.md), [CRA information for users](evaluate/cra-user-information.md) |

## How this book is organised

The parts follow the Diátaxis framework (<https://diataxis.fr/>):

- **Tutorials** take you through a first result, step by step.
- **How-to guides** answer one task each, for integrators and for operators.
- **Reference** lists the configuration, the command lines, the FHIR
  extensions, the problem types and the other facts you look up, generated
  from the code where it can be.
- **Explanation** says how the parts fit and which specification governs
  each.
- **Compliance** holds the Cyber Resilience Act material.
- **Contribute** covers building, testing and the checks a change passes.

FerroTERM is one product of the FerroHEALTH family, and Cadasto B.V. is the
manufacturer of every product. The manufacturer, the shared security policy
and support period, and the licensing and trademark terms every product
shares are written once, in the
[FerroHEALTH book](https://ferrohealth.eu/docs/).
