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

- **Reads the token endpoint from the service**, at
  `<base>/fhir/.well-known/smart-configuration`, so a realm move needs no code
  change.
- **Logs in with the documented grant.** The password grant with the
  `cli_client` client and a personal account is what the manual describes. A
  deployment holding a client secret gets the client-credentials grant first,
  with the password grant as the fallback.
- **Keeps the token fresh.** The access token is refreshed inside a 60 second
  margin, and a refused refresh, which is what a spent 24 hour refresh token
  gives, becomes a fresh login from the stored credentials. A run on any day
  succeeds with nobody present.
- **Subscribes by canonical identifier.** An entry for a system outside the
  subscription is reported as skipped, and a system the service offers only as
  Ontoserver's binary index is reported as not syndicable, both with the
  reason.
- **Corrects one thing.** A resource file whose `experimental` element is the
  string `"true"` or `"false"` gets the boolean FHIR declares, and the
  correction is returned beside the file. A file that needs nothing lands
  byte-identical.

## Credentials

The account is the deployment's, and it never enters a configuration body. Set
`credentials` to `{"from": "environment"}` and export
`FERROTERM_NTS_USERNAME` and `FERROTERM_NTS_PASSWORD` (plus
`FERROTERM_NTS_CLIENT_ID` or `FERROTERM_NTS_CLIENT_SECRET` where they differ
from the defaults), or to `{"from": "file", "path": "…"}` and mount a JSON file
with the same field names. No rendering of the configuration, the credentials,
or the source prints a secret.

## Another service on the same software

`base_url` is configuration, so an Ontoserver deployment with the same
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
