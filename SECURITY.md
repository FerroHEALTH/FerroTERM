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
newest release (Art 13(10)). Art 13(10) lets the manufacturer deliver security
updates only in the version last placed on the market, provided users of
earlier versions receive it free of charge, which is how every FerroTERM
upgrade is delivered. The end month of the commitment for a release (month and
year, Art 13(19) and Annex II point 7) is printed in its release notes: it is
the date until which Cadasto B.V. keeps shipping fixed releases, and the
release itself is supported only while it is the newest.

| Artifact | Supported version | Where fixes ship |
| --- | --- | --- |
| The `ferroterm` and `ferroterm-build` binaries (release tarballs) | the newest published release | the newest release |
| The server and console container images | the newest published release | the newest release |
| The library crates on crates.io | the newest published release | the newest release |

`main` is unreleased and best effort.

**A security fix ships alone, or the release says why not.** A fixed
vulnerability ships in a security-only patch release, as Annex I Part II(2)
asks where technically feasible. When a functional change has to ride with it,
the release's `### Security` section opens with the reason, and the release
notes repeat it.

To hear of new releases and fixes, subscribe to the release feed,
`https://github.com/FerroHEALTH/FerroTERM/releases.atom`, and poll the
published advisories,
`GET https://api.github.com/repos/FerroHEALTH/FerroTERM/security-advisories`.

## Reporting a vulnerability

**Do not open a public issue for a security vulnerability.**

This route covers the products this repository publishes: the FerroTERM
server and its tools (release tarballs), the `ferroterm` and
`ferroterm-console` container images, and the library crates on crates.io.
Report privately, through either route:

- [GitHub private vulnerability reporting](https://github.com/FerroHEALTH/FerroTERM/security/advisories/new)
  ("Report a vulnerability" on the repository's **Security** tab), which needs
  a GitHub account. It opens a private advisory visible only to you and the
  maintainer; GitHub documents the flow at
  <https://docs.github.com/en/code-security/security-advisories/guidance-on-reporting-and-writing-information-about-vulnerabilities/privately-reporting-a-security-vulnerability>.
- email to [info@cadasto.com](mailto:info@cadasto.com), the single point of
  contact of Cadasto B.V., the manufacturer (CRA Art 13(17)), with "FerroTERM
  vulnerability" in the subject. No account is needed. The address has no
  published encryption key, so send the details you are comfortable sending by
  email and say that more is available; we will agree a channel for the rest.

Both routes reach the same people and the same procedure. Describe the issue,
the affected version or commit, and a reproduction if you have one. If you
have seen the vulnerability exploited, say so. The family policy lists what a
report should carry.

**Never include SNOMED CT or other licensed terminology content in a report.**
A reproduction is easy to build from a licensed release by accident; use the
shaped, synthetic content the test fixtures use, or describe the request.

## Response times

FerroTERM is solo-maintained, so these are best effort rather than a
contractual service-level agreement: an acknowledgement within about 5
business days, and an initial assessment (is it valid, how severe) within
about 10. A confirmed vulnerability is fixed as a priority in a new patch
version; the family policy says how that release and its advisory reach you.

## Security advisories

A report, or a vulnerability we find ourselves, gets a private draft GitHub
security advisory on this repository on receipt; it is the record until the
fix ships. Fixed vulnerabilities in FerroTERM, and dependency or base-image
fixes that change what a shipped artefact contains, are published as those
advisories, in the same hour as the release that fixes them. The `### Security`
entry for the fix in [`CHANGELOG.md`](CHANGELOG.md) carries the advisory's
`GHSA-` identifier, and the release notes repeat it. The advisories are
machine-readable through the GitHub REST API
(`GET /repos/FerroHEALTH/FerroTERM/security-advisories`). The steps are in
[`docs/post-market.md`](docs/post-market.md).

## Scope

In scope: the FerroTERM server and libraries in this repository, its build and
release pipeline, and its published artifacts.

Out of scope: vulnerabilities in third-party dependencies without a
FerroTERM-specific impact (report those upstream; Dependabot and `cargo deny`
track advisories here), and issues that require an already-compromised host or
a misconfiguration outside FerroTERM's control.

- A dependency advisory the advisory gate accepts, and any finding in an
  inherited container layer we have argued does not affect FerroTERM, is
  published as an OpenVEX statement under [`security/vex/`](security/vex/),
  with the justification and an impact statement you can check. If you think
  one of those arguments is wrong, that is a valid report.
- Every release carries its vulnerability record
  (`ferroterm-<tag>-vulnerability-record.json`): a scan of both images and the
  server tarballs at every severity, each finding joined with the statement
  that judges it. A release with an unjudged finding is not published, and
  its images are scanned before they are tagged.

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
