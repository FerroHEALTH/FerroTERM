<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Post-market procedures

The working checklist for FerroTERM's side of the post-market procedure that
Cadasto B.V., the manufacturer of each tagged FerroTERM release, runs for every
FerroHEALTH product. The procedure itself (the channels, how a report is
classified, the CRA Art 14 notification, when an advisory's details may wait,
reporting to the maintainer of an integrated component, and the cessation of
operations) is the manufacturer's and is published in the FerroHEALTH book:
[Post-market](https://ferrohealth.eu/docs/post-market.html). The duties come
from Regulation (EU) 2024/2847, the Cyber Resilience Act.

This file keeps the steps bound to this repository: its tracker, its
advisories and its release lane.

## Who does what in this repository

| Who | What |
|---|---|
| Cadasto B.V., the manufacturer | Receives reports at `info@cadasto.com`, decides whether a vulnerability is actively exploited, and signs every report to an authority (the FerroHEALTH procedure). |
| The maintainer, Ruben Talstra | Reads FerroTERM's private vulnerability reports and the reports Cadasto B.V. forwards from `info@cadasto.com`, drafts the advisory, prepares the fixing release, and drafts the reports for Cadasto B.V. to send. |

FerroTERM's channels are GitHub private vulnerability reporting on this
repository ([`SECURITY.md`](../SECURITY.md)) and `info@cadasto.com` with the
subject "FerroTERM vulnerability". An emailed vulnerability report gets a draft
security advisory on this repository on receipt, exactly as a private GitHub
report does, and the clocks run from the email's arrival.

## A fixed vulnerability's advisory

Every fixed vulnerability in FerroTERM gets a GitHub security advisory on this
repository, published with the release that fixes it: one in FerroTERM's own
code, and one for a dependency or base-image fix that changes what a shipped
artefact (a binary, an image, a published crate) contains or how it behaves. A
dependency finding that an OpenVEX statement under `security/vex/` shows does
not affect the artefact gets no advisory. What the advisory carries, and when
its technical details may wait, is the FerroHEALTH
[security policy](https://ferrohealth.eu/docs/security.html).

1. **On the report or the finding:** open a draft advisory on this repository
   (Security, Advisories, New draft security advisory). It is the record until
   publication. Its affected product is `ferroterm`, and its affected range
   ends at the newest published release.
2. **Ship the fix** as a security-only patch release. If a functional change
   has to ride with it, write the reason in a
   `changelog.d/*.security-reason.md` fragment (`changelog.d/README.md`,
   `.claude/rules/changelog.md`).
3. **The changelog fragment** (`changelog.d/<issue>-<slug>.security.md`)
   carries the advisory's `GHSA-` identifier. CI refuses a Security entry
   without one.
4. **Publish the advisory** when the release is published, with the patched
   version filled in, and request a CVE through it where the vulnerability
   warrants one.

## Vulnerabilities in integrated components

A vulnerability the maintainer confirms in a component FerroTERM integrates (a
Rust crate, the distroless base image, a vendored asset) is reported upstream
as the FerroHEALTH procedure says (CRA Art 13(6)). For a Rust crate the channel
is its `SECURITY.md` or private vulnerability reporting on its repository, then
a RustSec advisory (`rustsec/advisory-db`) once it can be public, so that
`cargo audit` and `cargo deny` users see it. FerroTERM's own side is handled
like any other vulnerability: a draft advisory and a fixing release when
FerroTERM is affected, or an OpenVEX statement when it is not.

The `upstream-report` label on this tracker is for defects in the published
specifications, not for component vulnerabilities.
