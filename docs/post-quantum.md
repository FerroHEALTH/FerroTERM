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
- **RFC 9964:** ML-DSA for JOSE and COSE (key type `AKP`). **RFC 9881:**
  ML-DSA in X.509.

A row is `safe` when every key it depends on is established or verified by
a post-quantum or hybrid mechanism, or when it is a symmetric primitive at
128 bits or more. A row whose post-quantum form has no standard yet counts
as not safe and stays out of the badge, which counts the safe rows of the
rows a post-quantum standard covers; the `no standard yet` rows of the table
below are the rest. The badge is a self-assessment and never a
certification.

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
authenticate a TLS peer stay classical. RFC 9881 (October 2025, Proposed
Standard) defines ML-DSA keys and signatures in X.509 certificates. The TLS
1.3 signature schemes for ML-DSA (`mldsa44` 0x0904, `mldsa65` 0x0905,
`mldsa87` 0x0906) are defined by draft-ietf-tls-mldsa-06, which the IESG
approved and which waits in the RFC Editor queue, not yet an RFC
(<https://datatracker.ietf.org/doc/draft-ietf-tls-mldsa/>, read
2026-11-27). The row stays `no` while that document is a draft and the
operator's certificates decide the algorithm. The MD5 checksum row stays
`no` because MD5 is broken without a quantum computer; FerroTERM checks it
only where a feed publishes nothing else.

## JOSE signatures

`ferrohealth-smart` 0.36.0 signs and verifies every JWS in its `jws`
module, ML-DSA-44, ML-DSA-65 and ML-DSA-87 (RFC 9964, `AKP` keys) among the
algorithms. What it accepts on a verification is the list it is given:

- **Access tokens and DPoP proofs:** an issuer's `algorithms` and
  `[auth.dpop] algorithms` default to RS256, RS384, PS256, PS384, ES256 and
  ES384. An ML-DSA parameter set is accepted where a list names it; it is
  not accepted by default, which this page said before.
- **The console's sign-in ID Token:** the relying party's default is RS256
  to PS512, ES256, ES384 and EdDSA; the console adds the three ML-DSA
  parameter sets to it.
- **Client assertions:** a key signs with the algorithm of its kind, an
  ML-DSA key (RFC 9881 §6) with its parameter set and an RSA key with RS384.
- **DPoP proofs FerroTERM makes:** the crate's `DpopKey` reads a P-256,
  P-384 or ML-DSA key (RFC 9881 §6) and signs with the algorithm of its
  kind, ES256, ES384 or the key's ML-DSA parameter set.

`[jose] post_quantum_only = true`, in the server's and the sync service's
configuration, narrows every algorithm list of a JWT issuer (and of
`[auth.dpop]` on the server) left at its default to the three ML-DSA
parameter sets, and stops the start when a list names another algorithm.
The console's own `[jose] post_quantum_only = true` accepts an ID Token
signed with ML-DSA alone. All three are off by default, for the issuers
that still sign classically.

The badge counts the JOSE rows in that configuration: `post_quantum_only`
on and ML-DSA keys for the client assertions and the DPoP proofs, as the
book's examples set them. A deployment that leaves the settings off accepts the classical
algorithms the rows name, and those rows are then not post-quantum safe for
it.

## The inventory

<!-- post-quantum table -->
| Use | Algorithm | Safe | Post-quantum standard | Location |
|---|---|---|---|---|
| TLS 1.3 key exchange of the listeners: the server's FHIR and admin listeners, the sync service's admin listener and the console | X25519MLKEM768 first; X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `ferrohealth-server tls, app/ferroterm-server/src/lib.rs, app/ferroterm-sync/src/cli.rs, app/ferroterm-console/src/server.rs` |
| TLS 1.3 key exchange of the outbound clients FerroTERM builds: the console's clients of the server, the sync service and the admin listener, the sync service's reload, revalidation and webhook clients, and the NTS add-on | X25519MLKEM768 first; X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `ferrohealth-smart client outbound, app/ferroterm-console/src/server.rs, app/ferroterm-sync/src/reload.rs, app/ferroterm-sync/src/revalidate.rs, app/ferroterm-sync/src/webhook.rs, addons/nts/src/source.rs` |
| TLS 1.3 key exchange of the WHO ICD-API client of the offline build | X25519MLKEM768 first (rustls prefer-post-quantum); X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `crates/icd11/src/api.rs, tools/ferroterm-build/Cargo.toml` |
| TLS 1.3 key exchange of the family clients: the issuer key sets, the console's OpenID Provider, the token endpoints, the OTLP export and the FerroPULSE usage report | X25519MLKEM768 first (rustls prefer-post-quantum); X25519, P-256, P-384 for a peer without it | yes | RFC 10024 | `ferrohealth-smart, ferrohealth-session, ferrohealth-telemetry, ferrohealth-usage-report` |
| TLS 1.2 key exchange where min_version lowers the floor | ECDHE over X25519, P-256 or P-384 | no | no standard yet | `ferrohealth-server tls (min_version)` |
| TLS server and client certificate authentication | ECDSA, RSA or Ed25519, as the operator's certificates hold | no | RFC 9881; draft-ietf-tls-mldsa-06 in the RFC Editor queue | `ferrohealth-server tls, ferrohealth-server tls_client` |
| TLS record protection | AES-128-GCM, AES-256-GCM, ChaCha20-Poly1305 | yes | NIST IR 8547 4.1.3 | `ferrohealth-server tls` |
| Access token and DPoP proof verification with [jose] post_quantum_only = true | ML-DSA-44, ML-DSA-65 or ML-DSA-87 (AKP) alone; off (the default), RS256, RS384, PS256, PS384, ES256 and ES384, and an ML-DSA parameter set where an algorithms list names it | yes | RFC 9964 | `ferrohealth-smart resource, app/ferroterm-server/src/config.rs, app/ferroterm-server/src/smart/mod.rs, app/ferroterm-sync/src/auth.rs` |
| Client assertions of the outbound client credentials grant (private_key_jwt) with an ML-DSA key, as the examples configure it | ML-DSA-44, ML-DSA-65 or ML-DSA-87, the parameter set of the key (ML-DSA-65 in the examples); with a classical key RS384 by default, RS256 to PS512, ES256, ES384 or EdDSA | yes | RFC 9964 | `ferrohealth-smart client assertion, app/ferroterm-sync/src/revalidate.rs, addons/nts/src/source.rs` |
| DPoP proofs of the outbound client credentials grant with an ML-DSA DPoP key, as the examples configure it | ML-DSA-44, ML-DSA-65 or ML-DSA-87, the parameter set of the key (ML-DSA-65 in the examples); with a P-256 or P-384 key ES256 or ES384 | yes | RFC 9964 | `ferrohealth-smart client dpop, app/ferroterm-sync/src/reload.rs, addons/nts/src/source.rs` |
| Console sign-in ID token verification with [jose] post_quantum_only = true | ML-DSA-44, ML-DSA-65 or ML-DSA-87 alone; off (the default), those beside RS256, RS384, RS512, PS256, PS384, PS512, ES256, ES384 and EdDSA | yes | RFC 9964 | `ferrohealth-smart oidc, app/ferroterm-console/src/config/mod.rs, app/ferroterm-console/src/server.rs` |
| Console sign-in PKCE challenge | SHA-256 (S256) | yes | NIST IR 8547 4.1.3 | `ferrohealth-smart oidc, app/ferroterm-console/src/server.rs` |
| Basic password hashing | Argon2id | yes | NIST IR 8547 4.1.3 | `ferrohealth-smart basic, app/ferroterm-server/src/smart/mod.rs, app/ferroterm-sync/src/auth.rs` |
| Syndication download checksums where the feed publishes a SHA-256 | SHA-256 | yes | NIST IR 8547 4.1.3 | `crates/terminology-syndication/src/download.rs, crates/terminology-syndication/src/model.rs` |
| Syndication download checksums where the feed publishes only sct:md5Hash | MD5 | no | NIST IR 8547 4.1.3 | `crates/terminology-syndication/src/download.rs, crates/terminology-syndication/src/model.rs` |
| FHIR package pin of the offline build | SHA-256 | yes | NIST IR 8547 4.1.3 | `tools/ferroterm-build/src/package.rs` |
| crates.io publishing | Trusted Publishing (OIDC token from GitHub, RS256) | no | no standard yet | `.github/workflows/release.yml, .github/workflows/publish-crates.yml` |
| Documentation site deployment | GitHub Pages deployment (OIDC token from GitHub, RS256) | no | no standard yet | `.github/workflows/docs.yml` |
| Release provenance and SBOM attestations | Sigstore attestation (ECDSA P-256) | no | no standard yet | `.github/workflows/release-build.yml, .github/workflows/release-image.yml, .github/workflows/release-console.yml` |
<!-- end of the post-quantum table -->
