<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Post-quantum inventory

This page lists every cryptographic use in FerroTERM, its services, its
offline build and its release workflows, the algorithm each uses, and
whether it resists a cryptographically relevant quantum computer. The
machine-readable copy is `conformance/post-quantum.tsv`: after its `#`
comment lines, a header and one row per use with five tab-separated
columns: `use`, `algorithm`, `safe` (`yes` or `no`), `pq_standard` (the
standard that defines the post-quantum form, or `no standard yet`) and
`location`. A change that adds, removes or changes a cryptographic use
updates the file in the same pull request. `scripts/checks/post-quantum.sh`
rewrites the table below from it, and CI fails when the file is malformed,
when the table disagrees with it, or when a file a location names is
missing. The README badge is drawn from the same file by the family's
shared badge helper (`scripts/checks/badge-logos/logo.sh`) when the
documentation site is published.

## The sources

- **NIST IR 8547 (final):** ECDSA, EdDSA and RSA signatures (§4.1.1) and
  ECDH, DH and RSA key establishment (§4.1.2) are deprecated after 2030 and
  disallowed after 2035. Symmetric primitives with at least 128 bits of
  classical security, AES-128 and up and SHA-256 among them, stay (§4.1.3).
- **The EU coordinated implementation roadmap (NIS Cooperation Group,
  v1.1):** a use case is high-risk when its confidentiality must hold for
  10 years or more, which health data needs, and high-risk use cases
  complete the transition by 31.12.2030 (§4.2). After 2030 they use no
  quantum-vulnerable public-key mechanism stand-alone, and standardised
  hybrid solutions are recommended (§4.1).
- **RFC 10024:** the TLS 1.3 hybrid group X25519MLKEM768 (codepoint
  0x11EC), within the hybrid design of RFC 9954.
- **RFC 9964:** ML-DSA for JOSE and COSE (key type `AKP`).

A row is `safe` when every key it depends on is established or verified by
a post-quantum or hybrid mechanism, or when it is a symmetric primitive at
128 bits or more. A row whose post-quantum form has no standard yet counts
as not safe, and the badge counts the safe rows of all rows. The badge is a
self-assessment and never a certification.

## What FerroTERM does

The listeners of the server, the sync service and the console come from
`ferrohealth-server` 0.12.4, which offers X25519MLKEM768 first in the TLS
1.3 key exchange, then X25519, P-256 and P-384 for a peer that lacks the
hybrid group. Its outbound `TlsClient` takes the same order, and every
outbound client FerroTERM builds goes through it by way of
`ferrohealth-smart`'s `OutboundClient`. It also enables rustls's
`prefer-post-quantum` feature, so the clients built on the default provider
take the same order: the family clients (issuer key sets, the console's
OpenID Provider, token endpoints, the OTLP export and the usage report) and
the WHO ICD-API client of `ferroterm-build`, which turns the feature on in
its own manifest because it links no family crate.

Tests show both server listeners agreeing on X25519MLKEM768 with a client
that offers it and on X25519 with one that does not
(`app/ferroterm-server/tests/it/tls.rs`). They show the console's three
clients, the sync service's admin and webhook clients and the ICD-API
client preferring X25519MLKEM768 against a peer that offers X25519 first
and the hybrid group second, and agreeing on X25519 with a classical-only
peer (`app/ferroterm-console/tests/it/post_quantum.rs`,
`app/ferroterm-sync/tests/it/post_quantum.rs`,
`tools/ferroterm-build/tests/it/post_quantum.rs`, with the peer in
`tools/ferroterm-testkit/src/kx.rs`).

The key exchange protects what a session carries against an attacker who
records it now and decrypts it later. The certificate signatures that
authenticate a TLS peer stay classical: no TLS 1.3 signature scheme for
ML-DSA is published as an RFC yet, and the operator's certificates decide
the algorithm. The resource guard accepts access tokens signed with ML-DSA
(RFC 9964) beside the classical algorithms, and a client assertion signs
with ML-DSA when its configured key is an ML-DSA key; both rows stay `no`
while a classical signature is still accepted or configured. The MD5
checksum row stays `no` because MD5 is broken without a quantum computer;
FerroTERM checks it only where a feed publishes nothing else.

## The inventory

<!-- post-quantum table -->
| Use | Algorithm | Safe | Post-quantum standard | Location |
|---|---|---|---|---|
| TLS 1.3 key exchange of the listeners: the server's FHIR and admin listeners, the sync service's admin listener and the console | X25519MLKEM768 first; X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `ferrohealth-server tls, app/ferroterm-server/src/lib.rs, app/ferroterm-sync/src/cli.rs, app/ferroterm-console/src/server.rs` |
| TLS 1.3 key exchange of the outbound clients FerroTERM builds: the console's clients of the server, the sync service and the admin listener, the sync service's reload, revalidation and webhook clients, and the NTS add-on | X25519MLKEM768 first; X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `ferrohealth-smart client outbound, app/ferroterm-console/src/server.rs, app/ferroterm-sync/src/reload.rs, app/ferroterm-sync/src/revalidate.rs, app/ferroterm-sync/src/webhook.rs, addons/nts/src/source.rs` |
| TLS 1.3 key exchange of the WHO ICD-API client of the offline build | X25519MLKEM768 first (rustls prefer-post-quantum); X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `crates/icd11/src/api.rs, tools/ferroterm-build/Cargo.toml` |
| TLS 1.3 key exchange of the family clients: the issuer key sets, the console's OpenID Provider, the token endpoints, the OTLP export and the FerroPULSE usage report | X25519MLKEM768 first (rustls prefer-post-quantum); X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `ferrohealth-smart, ferrohealth-session, ferrohealth-telemetry, ferrohealth-usage-report` |
| TLS 1.2 key exchange where min_version lowers the floor | ECDHE over X25519, P-256 or P-384 | no | no standard yet | `ferrohealth-server tls (min_version)` |
| TLS server and client certificate authentication | ECDSA, RSA or Ed25519, as the operator's certificates hold | no | no standard yet | `ferrohealth-server tls, ferrohealth-server tls_client` |
| TLS record protection | AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305 | yes | NIST IR 8547 4.1.3 | `ferrohealth-server tls` |
| Access token and DPoP proof verification | ML-DSA-44, ML-DSA-65 or ML-DSA-87 (AKP) beside RS256, RS384, PS256, PS384, ES256 and ES384, as the issuer signs | no | RFC 9964 | `ferrohealth-smart resource, app/ferroterm-server/src/smart/mod.rs, app/ferroterm-sync/src/auth.rs` |
| Client assertions of the outbound client credentials grant (private_key_jwt) | the configured key's algorithm: RS384 by default, RS256 to PS512, ES256, ES384, EdDSA, or ML-DSA-44, ML-DSA-65 or ML-DSA-87 with an ML-DSA key | no | RFC 9964 | `ferrohealth-smart client assertion, app/ferroterm-sync/src/revalidate.rs, addons/nts/src/source.rs` |
| DPoP proofs of the outbound client credentials grant | ES256 or ES384 | no | RFC 9964 | `ferrohealth-smart client dpop, addons/nts/src/source.rs` |
| Console sign-in ID token verification | RS256, RS384, RS512, PS256, PS384, PS512, ES256, ES384, EdDSA | no | RFC 9964 | `ferrohealth-smart oidc, app/ferroterm-console/src/config/settings.rs` |
| Console sign-in PKCE challenge | SHA-256 (S256) | yes | NIST IR 8547 4.1.3 | `ferrohealth-smart oidc, app/ferroterm-console/src/server.rs` |
| Console session cookie in sealed mode | AES-256-GCM | yes | NIST IR 8547 4.1.3 | `ferrohealth-session sessions, app/ferroterm-console/src/config/settings.rs` |
| Basic password hashing | Argon2id | yes | NIST IR 8547 4.1.3 | `ferrohealth-smart basic, app/ferroterm-server/src/smart/mod.rs, app/ferroterm-sync/src/auth.rs` |
| Syndication download checksums where the feed publishes a SHA-256 | SHA-256 | yes | NIST IR 8547 4.1.3 | `crates/terminology-syndication/src/download.rs, crates/terminology-syndication/src/model.rs` |
| Syndication download checksums where the feed publishes only sct:md5Hash | MD5 | no | NIST IR 8547 4.1.3 | `crates/terminology-syndication/src/download.rs, crates/terminology-syndication/src/model.rs` |
| FHIR package pin of the offline build | SHA-256 | yes | NIST IR 8547 4.1.3 | `tools/ferroterm-build/src/package.rs` |
| crates.io publishing | Trusted Publishing (OIDC token from GitHub, RS256) | no | no standard yet | `.github/workflows/release.yml, .github/workflows/publish-crates.yml` |
| Documentation site deployment | GitHub Pages deployment (OIDC token from GitHub, RS256) | no | no standard yet | `.github/workflows/docs.yml` |
| Release provenance and SBOM attestations | Sigstore attestation (ECDSA P-256) | no | no standard yet | `.github/workflows/release-build.yml, .github/workflows/release-image.yml, .github/workflows/release-console.yml` |
<!-- end of the post-quantum table -->
