- Each release scans the `ferroterm` and `ferroterm-console` images before it
  tags them: both are pushed by digest, a vulnerability record scans them and
  the server's glibc tarballs at every severity, and the release is refused
  while a finding has no OpenVEX statement under `security/vex/` judging it
  (Regulation (EU) 2024/2847 Annex I Part I(2)(a)). The record,
  `ferroterm-<tag>-vulnerability-record.json`, and the raw Trivy reports are
  release assets, and the image tags and the published release follow only a
  passing record and a scan with no fixable HIGH or CRITICAL finding. The one
  accepted Rust advisory (`paste`, RUSTSEC-2024-0436) is published as the
  generated `security/vex/rust-advisories.openvex.json`, and a weekly scan of
  the published images applies it. The stale `deny.toml` exception for
  RUSTSEC-2026-0173 (`proc-macro-error2`, no longer in the dependency graph)
  is dropped.
