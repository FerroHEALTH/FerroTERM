<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# addon-nts

The Nationale Terminologie Server (NTS) source add-on: the first add-on under
`addons/*`, and the shape every later one follows. The service is Nictiz's
Dutch national terminology server
(<https://www.nictiz.nl/publicaties/nationale-terminologie-server-handleiding-voor-nieuwe-gebruikers/>),
an Ontoserver deployment whose syndication feed sits behind an OAuth 2 bearer
challenge.

No FHIR or SNOMED CT specification governs a syndication feed. The
specifications that do govern this crate are OAuth 2 (RFC 6749) for the grants,
SMART App Launch for where the token endpoint is discovered, and the FHIR
`boolean` type for the one content correction.

## What lives here, and what does not

- **Here:** the feed address and the base URL, the token machinery, where the
  credentials come from, the subscription by canonical identifier, and the
  `experimental` correction.
- **In `crates/terminology-syndication`:** the Atom dialect, the selection
  rules, the checksum-verified download, and the `Source` seam. Never copy a
  type from there into here; a gap in the model is a change there.
- **Never here:** the server, the console, another add-on, or anything that
  reads an artifact. An add-on takes `terminology-syndication` and leaf crates
  only.

## The rules this add-on holds

- **A credential never enters the configuration body.** `CredentialSource`
  names a file or the environment, and `Credentials` renders only which grants
  are configured. Any new field holding a secret gets the same treatment, and
  the test asserting that no rendering carries a secret covers it.
- **A token is renewed before it expires, never after.** The refresh runs
  inside the margin; a refused refresh logs in again from the stored
  credentials, so a run on any day succeeds with nobody present.
- **A correction is recorded or it did not happen.** `fixup::normalize`
  returns the list beside the bytes, and a file that needs nothing comes back
  byte-identical, so a run record can tell the two apart.
- **A second Ontoserver deployment is this add-on with another base URL.**
  Nothing may hard-code the host outside `config::NTS_BASE_URL`.

## Tests

`tests/it/` is one binary with a module per topic. Every test runs against
`wiremock`, never the network, and the clock is injected through
`auth::Clock`, so a run covering 25 hours takes milliseconds. The feed fixture
is synthetic: invented entry identifiers and no terminology content.

The feed itself is unverified against the live service until an account exists
(#581 stays open for that). Until then, treat every fact about entry formats as
what the fixture shapes, not as what the service serves.
