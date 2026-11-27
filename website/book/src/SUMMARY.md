<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Summary

[FerroTERM](introduction.md)

# Tutorials

- [Your first requests](tutorials/first-requests.md)

# How-to guides: integrate

- [The FHIR terminology API](integrate/fhir-api.md)
  - [Writing resources](integrate/persisted-resources.md)
  - [Batch requests and closure tables](integrate/batch-and-closure.md)
- [Value sets, implicit value sets, and ECL](integrate/ecl-value-sets.md)

# How-to guides: deploy and operate

- [Install and run](operate/install.md)
- [Loading code systems](operate/loading-snomed.md)
  - [Loading the EEHRxF code systems](operate/loading-eehrxf.md)
- [Reloading and replacing content](operate/reloading.md)
- [Upgrading](operate/upgrades.md)
- [Syncing from a national terminology service](operate/sync.md)
- [Behind a reverse proxy](operate/reverse-proxy.md)
  - [Running behind FerroSMART](operate/behind-ferrosmart.md)
- [The console](operate/console.md)
- [Hardware sizing](operate/hardware-sizing.md)
- [Verifying releases](operate/verifying-releases.md)

# Reference

- [Configuration](operate/configuration.md)
- [`ferroterm`](reference/ferroterm.md)
- [`ferroterm-build`](reference/ferroterm-build.md)
- [`ferroterm-sync`](reference/ferroterm-sync.md)
- [FHIR versions](integrate/fhir-versions.md)
- [FerroTERM's FHIR extensions](reference/fhir-extensions.md)
- [Code systems served](evaluate/code-systems.md)
- [Health, metrics, and request identifiers](operate/observability.md)
- [Problem types](operate/problems.md)
- [Usage report](operate/usage-report.md)
- [What stable means](evaluate/what-stable-means.md)
- [Licensing](evaluate/licensing.md)

# Explanation

- [How FerroTERM serves terminology](explanation/how-ferroterm-serves.md)
- [What FerroTERM is](evaluate/what-ferroterm-is.md)
- [Architecture at a glance](evaluate/architecture.md)
- [How it compares](evaluate/comparison.md)
- [Conformance](evaluate/conformance.md)
- [Benchmarks](evaluate/benchmarks.md)

# Compliance

- [CRA risk assessment](evaluate/cra-risk-assessment.md)
- [CRA information for users](evaluate/cra-user-information.md)

# Contribute

- [Build and test](contribute/build-and-test.md)
- [The FHIR model](contribute/codegen.md)
- [The performance gate](contribute/performance-gate.md)

---

[Changelog](changelog.md)
