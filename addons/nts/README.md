<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# addon-nts

The Nationale Terminologie Server (NTS) source add-on for the FerroTERM sync
service.

The NTS is the Dutch national terminology service, run by
[Nictiz](https://www.nictiz.nl/publicaties/nationale-terminologie-server-handleiding-voor-nieuwe-gebruikers/).
It publishes its downloadable content in the Atom syndication dialect
`terminology-syndication` reads, and the whole feed, the listing included, sits
behind an OAuth 2 bearer challenge. This crate is that service's identity: the
feed address, the token machinery, the subscription, and the one content
correction its files need.

## What it does

- **Authenticates as a client.** The add-on uses the OAuth 2.0 client
  credentials grant (RFC 6749 §4.4) through `ferrohealth-smart`'s
  `TokenSource`, with a confidential client Nictiz issues. The service's
  issuer publishes the grant, and its own examples use it
  (<https://github.com/terminologieserver/ontoserver_authentication_examples>).
  The password grant the manual describes for a personal account is not used:
  RFC 9700 §2.4 says it MUST NOT be.
- **Reads the token endpoint from the issuer**, the
  `nts.credentials.oauth.issuer` key (by default the service's realm), through its OpenID Connect Discovery metadata, so a
  move of the token endpoint needs no code change.
- **Keeps the token fresh.** The token is cached and a new one is fetched
  inside a 60 second margin before it expires, so a run on any day succeeds
  with nobody present.
- **Subscribes by canonical identifier.** An entry for a system outside the
  subscription is reported as skipped, and a system the service offers only as
  Ontoserver's binary index is reported as not syndicable, both with the
  reason.
- **Corrects one thing.** A resource file whose `experimental` element is the
  string `"true"` or `"false"` gets the boolean FHIR declares, and the
  correction is returned beside the file. A file that needs nothing lands
  byte-identical.

## Credentials

The client is the deployment's, and it never enters the add-on's
configuration block. The synchronisation service reads the service's
address and the client from its `[nts]` endpoint table, which starts from
`addon_nts::config::endpoint` (`FERROTERM_SYNC__NTS__URL`,
`FERROTERM_SYNC__NTS__CREDENTIALS__OAUTH__CLIENT_ID`,
`FERROTERM_SYNC__NTS__CREDENTIALS__OAUTH__CLIENT_SECRET`, the secret also as
a mounted `_file`), and hands the table to `NtsSource::new`, which builds the
family outbound client (`ferrohealth_smart::client::outbound`) from it: the
HTTP engine with the table's timeouts and TLS, the token source, and the
retry of a call that could not reach the service or met a `5xx`. No
rendering of the configuration or the source prints a secret.

## Another service on the same software

`[nts] url` is configuration, so an Ontoserver deployment with the same
authentication shape is this add-on pointed elsewhere. Anything specific to
that operator, a different fix-up or another feed layout, belongs in its own
add-on.

## Where it sits

`addon-nts` is one member of
[FerroTERM](https://github.com/FerroHEALTH/FerroTERM), a pure-Rust FHIR
terminology server for SNOMED CT, LOINC, and other clinical code systems. An
add-on is compiled into the sync service and is never published to crates.io.
It depends on `terminology-syndication` and leaf crates only.

## Licence

Business Source License 1.1 (the repository `LICENSE`): free to read, build,
modify, and redistribute, free for non-production use and for non-commercial
production use; commercial production use needs a licence from the Licensor;
each version becomes Apache License 2.0 four years after it is published.
Clinical terminology content is licensed by its publisher and is never part of
this crate.
