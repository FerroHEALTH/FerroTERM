<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Security Policy

FerroTERM is a read-oriented FHIR terminology server. This document says which
versions receive security fixes and how to report a vulnerability privately.

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
channel before sending any details; the maintainer's profile is
<https://github.com/rubentalstra>.

Please include, where you can:

- the affected version, tag, or commit;
- a description of the impact (what an attacker can do);
- steps to reproduce, a proof of concept, or a failing request;
- any suggested remediation.

## What to expect

This is a solo-maintained project, so timelines are best effort rather than a
contractual SLA:

- **Acknowledgement** of your report within about 5 business days.
- **An initial assessment** (is it valid, how severe) within about 10 business
  days.
- **A fix or a mitigation plan** for a confirmed vulnerability as a priority,
  released as a new patch version. A published release is immutable, so fixes
  ship forward as a new version rather than by re-tagging.

We will keep you informed through the private advisory and, with your consent,
credit you when the advisory is published. Please give us a reasonable window
to release a fix before any public disclosure (coordinated disclosure).

## Scope

In scope: the FerroTERM server and libraries in this repository, its build and
release pipeline, and its published artifacts.

Out of scope: vulnerabilities in third-party dependencies without a
FerroTERM-specific impact (report those upstream; Dependabot and `cargo deny`
track advisories here), and issues that require a already-compromised host or
misconfiguration outside FerroTERM's control.

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
