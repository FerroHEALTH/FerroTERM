<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# icd11

The ICD-11 reader. Hand-written; the WHO ICD-API (version 2, the local
deployment the licence allows) and the HL7 terminology ecosystem test cases for
ICD-11 are the authority, with THO's ICD-11 page for identity.

- The three code systems are the Foundation (`http://id.who.int/icd/entity`),
  the MMS linearization (`http://id.who.int/icd/release/11/mms`), and the ICF
  linearization (`http://id.who.int/icd/release/11/icf`); each is read from
  the JSON the API serves for its entities, cached as files so a build never
  needs the API again.
- The licence (CC BY-ND 3.0 IGO) requires code, title, and URI to travel
  together and forbids passing on derived content; the cache and the built
  artifacts stay local, and fixtures are shaped like the API's JSON with
  invented entities.
- The `api` module walks a deployment and is the only part that speaks HTTP, so
  it sits behind the `api` cargo feature, off by default and enabled only by
  `tools/ferroterm-build`. The server binary carries no HTTP client, which
  `scripts/checks/no-client-in-server.sh` measures.
- Postcoordination expressions are parsed here (`&` values on a stem, `/`
  between cluster members, ICF's `.` for qualifiers, the URI form); their
  validation against the axes needs the built hierarchy and lives in the
  provider.
