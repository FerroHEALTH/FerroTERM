<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Security Policy

FerroTERM is a read-oriented FHIR terminology server. This document carries
what is particular to FerroTERM: the versions that receive security fixes,
where to report, the response times it commits to, its scope, and how to verify
a release.

The vulnerability-handling policy every FerroHEALTH product shares is on one
page of the family book, written by the manufacturer, Cadasto B.V.:
<https://ferrohealth.eu/docs/security.html>. It covers what happens after you
report, coordinated disclosure and credit, how a fix and its advisory reach
you, the support period, and what the manufacturer reports to the authorities
under the Cyber Resilience Act.

## Supported versions

Only the newest release is supported. When a new release is published, every
earlier release stops receiving fixes: there are no maintenance branches, no
long-term-support line and no backports. If you are not on the newest
release, upgrade; upgrading is free. Under the Cyber Resilience Act, Cadasto
B.V. commits to keep publishing security fixes for at least five years after
each release (Regulation (EU) 2024/2847 Art 13(8)) and ships them only in the
newest release (Art 13(10)).

| Artifact | Supported version | Where fixes ship |
| --- | --- | --- |
| The `ferroterm` and `ferroterm-build` binaries (release tarballs) | the newest published release | the newest release |
| The server and console container images | the newest published release | the newest release |
| The library crates on crates.io | the newest published release | the newest release |

`main` is unreleased and best effort.

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.** Report it
privately through GitHub's private vulnerability reporting:

1. Open <https://github.com/FerroHEALTH/FerroTERM/security/advisories/new>,
   or go to the repository's **Security** tab and choose **Report a
   vulnerability**.
2. Describe the issue, the affected version or commit, and a reproduction if
   you have one.
3. Submit the report. GitHub documents the flow at
   <https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability>.

This opens a private advisory visible only to you and the maintainer. If you
cannot use GitHub's form, email <me@rubentalstra.nl> and ask for a private
channel before sending any details. The family policy lists what a report
should carry.

**Never include SNOMED CT or other licensed terminology content in a report.**
A reproduction is easy to build from a licensed release by accident; use the
shaped, synthetic content the test fixtures use, or describe the request.

## Response times

FerroTERM is solo-maintained, so these are best effort rather than a
contractual service-level agreement: an acknowledgement within about 5
business days, and an initial assessment (is it valid, how severe) within
about 10. A confirmed vulnerability is fixed as a priority in a new patch
version; the family policy says how that release and its advisory reach you.

## Scope

In scope: the FerroTERM server and libraries in this repository, its build and
release pipeline, and its published artifacts.

Out of scope: vulnerabilities in third-party dependencies without a
FerroTERM-specific impact (report those upstream; Dependabot and `cargo deny`
track advisories here), and issues that require an already-compromised host or
a misconfiguration outside FerroTERM's control.

## How releases are verified

Release binaries are built in an isolated reusable workflow (SLSA Build Level
3) and published with:

- a SHA-256 checksum (`.sha256sum`) for corruption detection;
- a Sigstore build-provenance bundle (`.sigstore.json` / `.intoto.jsonl`);
- a CycloneDX dependency SBOM (`.cdx.json`), and the dependency list embedded in
  the binary itself via `cargo auditable`.

Provenance can be verified with:

```
gh attestation verify ferroterm-<tag>-<target>.tar.gz \
  -R FerroHEALTH/FerroTERM \
  --signer-workflow FerroHEALTH/FerroTERM/.github/workflows/release-build.yml
```
