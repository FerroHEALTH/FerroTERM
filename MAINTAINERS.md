<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Maintainers and access continuity

This file is the roster and the honest answer to the question an enterprise
procurement review asks about any infrastructure software: *what happens if the
people who can ship a fix are unavailable?*

It is deliberately not aspirational. Everything below describes the project as
it is on the day you read it in git history, not a structure the project hopes
to grow into.

## Roster

| Person        | GitHub                                           | Role              | Since      |
|---------------|--------------------------------------------------|-------------------|------------|
| Ruben Talstra | [@rubentalstra](https://github.com/rubentalstra) | Maintainer (sole) | 2026-08-01 |

**The bus factor of this project is one.** There is one maintainer, one person
who publishes a release, and one person who accepts a pull request. The
repository lives in the [FerroHEALTH](https://github.com/FerroHEALTH) GitHub
organisation, so its organisation owners also hold admin access
(`GET /repos/FerroHEALTH/FerroTERM/collaborators` lists them). No second
maintainer exists.

The maintainer handles the technical side: code, review, releases, issues, and
security reports. Cadasto B.V. is the Licensor and copyright holder, and
handles the business side of FerroTERM, the commercial licence included: write
to [info@cadasto.com](mailto:info@cadasto.com) or use
<https://www.cadasto.com/contact/>.

Everything else in this file follows from that sentence, and no wording
elsewhere in the repository should be read as softening it. The path out is in
[GOVERNANCE.md](GOVERNANCE.md): becoming a maintainer is a defined route, and
it is open.

## Publishing identities and where they live

These are the credentials and configured identities that can put bytes in front
of a user. Naming them is the point: an inventory nobody has written down is an
inventory nobody can hand over.

| Identity                                     | What it publishes                                                    | Held by                                                       | Recovery if the holder is unavailable                                                                                                                                                        |
|----------------------------------------------|---------------------------------------------------------------------|--------------------------------------------------------------|--------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| The GitHub organisation `FerroHEALTH`        | everything: the repository, releases, issues, settings              | the organisation owners, the maintainer among them           | another organisation owner keeps access if the maintainer's account is unavailable                                                                                                         |
| The commit- and tag-signing key              | the verified signature on every commit and every release tag        | the maintainer, on his own hardware                          | none: the private key is not escrowed. A successor would publish a new key and re-establish trust from a signed statement on the repository; historical signatures stay verifiable regardless |
| `GITHUB_TOKEN` (ephemeral, per workflow run) | the GitHub release binaries, their SBOMs, and the Sigstore provenance | GitHub, minted per run; nothing is stored                    | not applicable: there is no credential to lose                                                                                                                                              |
| The `SONAR_TOKEN` repository secret          | nothing; it uploads code analysis to SonarQube Cloud                | the repository                                               | not applicable: a lost key costs a scan, not a release                                                                                                                                      |
| The GitHub Pages documentation site          | the landing page and the mdBook site                                | the `FerroHEALTH` organisation                              | another organisation owner                                                                                                                                                                 |

**The honest reading of that table:** every publishing identity terminates at
one person's GitHub account or one person's hardware. Keyless Sigstore signing
removes the *stored secret* risk for releases (there is no long-lived signing
key to leak), but it does not distribute the *authority*, which is still one
account's. That is the residual risk, and it is stated rather than mitigated
because no mitigation is currently available while the technical roles rest on
one maintainer.

## If the maintainer is unavailable

There is no succession plan that a document can create. What exists instead:

- **Nothing already published disappears.** Releases are immutable and their
  assets stay downloadable; a deployment already running is not affected by
  maintainer availability.
- **Nothing new ships.** No release and no security fix. The support window in
  [SECURITY.md](SECURITY.md) (only the newest release is supported) becomes, in
  that situation, no supported release at all.
- **The work is not lost.** The licence is the Business Source License 1.1,
  with Cadasto B.V. as the Licensor and copyright holder, which converts to
  Apache 2.0 four years after each version, the history is public, every
  gate is a committed script, and every design decision is in the tree or on
  the tracker. A fork is a complete and legitimate continuation, and the
  project's position is that it should be taken rather than waited on.
- **A vulnerability report has a fallback.** [SECURITY.md](SECURITY.md) states
  an acknowledgement window. If that window passes with no response, the
  situation is the ordinary one for unmaintained software, and public
  disclosure to protect other users becomes your call. That path does not
  depend on the maintainer.

If you depend on this software and that position is not acceptable to you (it
reasonably may not be), the mitigation is on your side of the boundary: pin a
version, keep a fork you can build, and budget for maintaining it. That is a
truthful answer, and it is more useful than a continuity plan with nobody
behind it.

## Adding a maintainer

The route is in [GOVERNANCE.md](GOVERNANCE.md). When someone takes it, this
file gains a row, [`.github/CODEOWNERS`](.github/CODEOWNERS) gains their handle
on the areas they own, and the table above gains a second holder wherever the
identity permits one. Those three edits are the whole mechanism.
