- Every TLS listener and outbound client prefers the hybrid key exchange
  X25519MLKEM768 (RFC 10024) and still connects to a classical-only peer,
  through `ferrohealth-server` 0.12.4 and rustls's `prefer-post-quantum`,
  which the offline build's ICD-API client now turns on as well. The
  post-quantum inventory (`docs/post-quantum.md`,
  `conformance/post-quantum.tsv`) lists every cryptographic use, CI holds
  it to the tree, and the README badge counts its safe uses.
