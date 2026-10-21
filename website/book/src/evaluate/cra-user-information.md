<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# CRA information for users

The Cyber Resilience Act (CRA, Regulation (EU) 2024/2847) has each product
"accompanied by the information and instructions to the user set out in Annex
II, in paper or electronic form" (CRA Art. 13(18)). This page carries that
information for FerroTERM, point by point in the order of Annex II. Each
section quotes what the point asks and gives FerroTERM's answer, or links the
page that already gives it.

Annex II opens: "At minimum, the product with digital elements shall be
accompanied by:". The product is each tagged FerroTERM release, as Cadasto
B.V. publishes it: the `ferroterm` and `ferroterm-build` binaries, the
`ferroterm` container image, the console's `ferroterm-viewer` binaries, site
bundle and image, and the source archive
([the scope of the risk assessment](cra-risk-assessment.md#scope-and-dates)).

<!-- toc -->

> [!WARNING]
> The quotations are from the Official Journal text of the
> [CRA](https://eur-lex.europa.eu/eli/reg/2024/2847/oj) at EUR-Lex. No
> conformity assessment has been carried out, no EU declaration of conformity
> exists and FerroTERM carries no CE marking. This page gives the information
> Annex II lists; it does not say that FerroTERM, or a deployment of it, meets
> the Regulation.

The information is written in English. Whether that meets CRA Art. 13(18) in
every Member State is a question the FerroHEALTH book records for counsel
([questions for counsel](https://ferrohealth.eu/docs/cra.html#questions-for-counsel)).

## 1. The manufacturer

"the name, registered trade name or registered trademark of the manufacturer,
and the postal address, the email address or other digital contact as well
as, where available, the website at which the manufacturer can be contacted;"

Cadasto B.V. is the manufacturer of every FerroHEALTH product. Its contact
details are in the FerroHEALTH book:
[the manufacturer](https://ferrohealth.eu/docs/manufacturer.html#contact-details).

## 2. The single point of contact for vulnerabilities

"the single point of contact where information about vulnerabilities of the
product with digital elements can be reported and received, and where the
manufacturer's policy on coordinated vulnerability disclosure can be found;"

- **Report a vulnerability** through
  [GitHub private vulnerability reporting](https://github.com/FerroHEALTH/FerroTERM/security/advisories/new),
  or by email as [`SECURITY.md`](https://github.com/FerroHEALTH/FerroTERM/blob/main/SECURITY.md#reporting-a-vulnerability)
  says. Do not open a public issue, and never put SNOMED CT or other licensed
  content in a report.
- **Receive information about vulnerabilities** from the GitHub security
  advisories of the repository, published with the release that fixes each
  one.
- **The coordinated vulnerability disclosure policy** is the manufacturer's,
  shared by every FerroHEALTH product:
  [security](https://ferrohealth.eu/docs/security.html) in the FerroHEALTH
  book.

## 3. Name, type and unique identification

"name and type and any additional information enabling the unique
identification of the product with digital elements;"

- **Name:** FerroTERM.
- **Type:** software. A FHIR terminology server for SNOMED CT, LOINC and other
  clinical code systems, with its offline index build tool and its browser
  console.
- **Version:** the release tag `vX.Y.Z`. The running server names it in its
  `ferroterm starting` log line and as `software.version` in every
  `CapabilityStatement` and `TerminologyCapabilities` it serves.
- **The artefacts of a version:** each release asset carries a checksum and a
  signed build provenance, and the images are identified by digest
  ([verifying releases](../operate/verifying-releases.md)).
- **Release date:** the date on the release's heading in the
  [changelog](https://github.com/FerroHEALTH/FerroTERM/blob/main/CHANGELOG.md),
  and `software.releaseDate` in the capability statements.

## 4. Intended purpose, security environment, essential functionalities and security properties

"the intended purpose of the product with digital elements, including the
security environment provided by the manufacturer, as well as the product's
essential functionalities and information about the security properties;"

- **Intended purpose and essential functionalities:** FerroTERM answers the
  HL7 FHIR terminology operations over the code systems a deployment loads,
  and keeps the `CodeSystem`, `ValueSet` and `ConceptMap` resources its
  clients write where the deployment names a database
  ([what FerroTERM is](what-ferroterm-is.md);
  [the basis of the risk assessment](cra-risk-assessment.md#the-basis-intended-purpose-use-and-environment)).
- **Security environment:** what FerroTERM relies on and does not provide
  itself: a reverse proxy that terminates TLS, authenticates readers where you
  want them authenticated, and limits rates, timeouts and request bodies; an
  authorization server for the write routes, the admin listener and the
  console's sign-in; and the protection of the disks that hold the indexes and
  the resource database
  ([behind a reverse proxy](../operate/reverse-proxy.md)).
- **Security properties:** each point of CRA Annex I Part I(2), with how
  FerroTERM implements it and the evidence, is in the
  [CRA risk assessment](cra-risk-assessment.md#annex-i-part-i2-point-by-point).

## 5. Circumstances that may lead to significant cybersecurity risks

"any known or foreseeable circumstance, related to the use of the product with
digital elements in accordance with its intended purpose or under conditions
of reasonably foreseeable misuse, which may lead to significant cybersecurity
risks;"

- **Use in accordance with the intended purpose:** the
  [risk register](cra-risk-assessment.md#how-the-risks-were-analysed) rates
  each risk with the shipped controls.
- **Reasonably foreseeable misuse:** the
  [misuse table](cra-risk-assessment.md#the-basis-intended-purpose-use-and-environment)
  lists each case and what remains. The ones to know before you deploy: the
  read surface asks for no credential, so a licence that limits who may read
  the content is kept by the proxy; the server speaks plain HTTP; turning
  authentication off with writes or the admin listener enabled hands both to
  anyone who reaches them; and the server has no rate limit or timeout of its
  own.

## 6. The EU declaration of conformity

"where applicable, the internet address at which the EU declaration of
conformity can be accessed;"

No EU declaration of conformity has been drawn up, so there is no address to
give. The declaration applies from 11 December 2027 (CRA Art. 71(2)), and the
FerroHEALTH book records the conformity route of FerroTERM as open
([how conformity is assessed](https://ferrohealth.eu/docs/cra.html#how-conformity-is-assessed-product-by-product)).
When it exists, it is published with each release it covers, and this section
gives its address.

## 7. Technical security support and the end of the support period

"the type of technical security support offered by the manufacturer and the
end-date of the support period during which users can expect vulnerabilities
to be handled and to receive security updates;"

- **Only the newest release is supported.** When a new release is published,
  every earlier release stops receiving fixes: there are no maintenance
  branches and no backports. Upgrading is free.
- **The end date:** Cadasto B.V. keeps publishing security fixes for at least
  five years after each release (CRA Art. 13(8)), so the support period of a
  release ends five years after the month of its release date, and a fixed
  release keeps coming until then. A release itself is supported only while
  it is the newest.
- **The type of support:** the manufacturer's, the same for every FerroHEALTH
  product ([how a fix reaches you](https://ferrohealth.eu/docs/security.html#how-a-fix-reaches-you)),
  with FerroTERM's supported artefacts in
  [`SECURITY.md` § Supported versions](https://github.com/FerroHEALTH/FerroTERM/blob/main/SECURITY.md#supported-versions).

## 8. Detailed instructions

"detailed instructions or an internet address referring to such detailed
instructions and information on:"

### 8(a) Secure use from commissioning on

"the necessary measures during initial commissioning and throughout the
lifetime of the product with digital elements to ensure its secure use;"

- **Commissioning:** verify what you downloaded
  ([verifying releases](../operate/verifying-releases.md)), install
  ([install and run](../operate/install.md)), put the server behind a reverse
  proxy that terminates TLS, and keep `/health`, `/metrics` and the admin
  listener off the public route
  ([behind a reverse proxy](../operate/reverse-proxy.md)). Before you enable
  writes or the admin listener, name an issuer and an audience in `[auth]`
  ([authentication](../operate/configuration.md#authentication)). Run the
  console with `secure_cookie = true` behind TLS
  ([the console](../operate/viewer.md)).
- **Throughout its lifetime:** follow the release feed and the security
  advisories, upgrade to each new release, and replace an index by building it
  in a new directory and renaming it into place
  ([reloading the served set](../operate/configuration.md#reloading-the-served-set)).

### 8(b) How changes to the product can affect the security of data

"how changes to the product with digital elements can affect the security of
data;"

- **Configuration.** Naming `FERROTERM_RESOURCES` turns on writes and
  `FERROTERM_ADMIN_LISTEN` the reload; both demand `[auth]`. Setting
  `[auth] enabled = false` removes the credential check from both. Binding the
  admin listener to a public address exposes the reload. Adding a user to
  `[[auth.basic.users]]` or a subject to `scopeless_subjects` grants writes.
- **Outbound data.** The usage report sends what
  [the usage report](../operate/usage-report.md#what-a-report-contains) lists,
  and `FERROTERM_USAGE_REPORT=off` stops it.
- **Upgrades.** An upgrade replaces the binaries or the image; the indexes,
  the resource database and the configuration stay where they are, and the
  release notes say when an index needs rebuilding.
- **Changes to the source.** A binary or image built from modified source is
  not a Cadasto B.V. release: its provenance does not verify, and the risk
  assessment does not describe it.

### 8(c) Installing security-relevant updates

"how security-relevant updates can be installed;"

Subscribe to the release feed
(`https://github.com/FerroHEALTH/FerroTERM/releases.atom`) and watch the
repository's security advisories; the server does not look for newer releases
itself. Verify the new release
([verifying releases](../operate/verifying-releases.md)), then replace the
binary or deploy the new image by digest and restart. The served indexes and
the resource database are carried over.

### 8(d) Secure decommissioning, and removing user data

"the secure decommissioning of the product with digital elements, including
information on how user data can be securely removed;"

FerroTERM keeps everything it holds in files and directories you name. To take
a deployment out of service and remove its data, stop the server and the
console, then delete:

- every directory `FERROTERM_INDEX` names, which hold the licensed content the
  build wrote, and the release files you built them from;
- the database file `FERROTERM_RESOURCES` names, which holds the resources
  your clients wrote and the closure tables;
- the directory `FERROTERM_STATE_DIR` names, which holds the usage report's
  instance id;
- the file `FERROTERM_AUTH_FILE` names, the Basic users' password hash files,
  and the console's configuration and client secret.

The console keeps its sessions in memory, so stopping it ends them. Remove
the copies FerroTERM cannot reach yourself: backups, volume snapshots, proxy
logs, and the data held by the clients that read from the server. Sanitising
the physical media is yours. To have the usage reports already sent erased,
follow [the usage report's privacy notice](../operate/usage-report.md#your-right-to-object).

### 8(e) Turning off automatic installation of security updates

"how the default setting enabling the automatic installation of security
updates, as required by Part I, point (2)(c), of Annex I, can be turned off;"

FerroTERM installs no update by itself, so there is no such setting to turn
off. The risk assessment records why the limb does not apply
([the justification](cra-risk-assessment.md#automatic-security-updates-annex-i-part-i2c)).

### 8(f) Information for an integrator

"where the product with digital elements is intended for integration into
other products with digital elements, the information necessary for the
integrator to comply with the essential cybersecurity requirements set out in
Annex I and the documentation requirements set out in Annex VII."

The server, the console and their images are not intended for integration
into another product; other software calls a running server over the FHIR
API. The library crates on crates.io are intended for integration, and each is
a product of its own: each crate's `README.md` and docs.rs page describe its
API, and [the crates on crates.io](cra-risk-assessment.md#the-crates-on-cratesio)
says which Annex I points each carries and which are left to the integrating
product.

## 9. The software bill of materials

"If the manufacturer decides to make available the software bill of materials
to the user, information on where the software bill of materials can be
accessed."

Cadasto B.V. makes the software bills of materials available. Each release
publishes a CycloneDX SBOM of the `ferroterm` and `ferroterm-build` binaries
as release assets and an SBOM attestation per platform of each image, and the
release binaries embed their dependency lists
([what a release carries](../operate/verifying-releases.md#what-a-release-carries)).
