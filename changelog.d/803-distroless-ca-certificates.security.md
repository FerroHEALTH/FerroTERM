- The server, sync and console images no longer trust the compromised
  eMudhra (emSign) intermediates in the system trust store (Debian
  DSA-6572-1, GHSA-7hfh-24x8-936h). The distroless base `gcr.io/distroless/static-debian13:nonroot`
  moves to index digest `d620678`, which ships `ca-certificates`
  20250419+deb13u1, in the `FROM` of `docker/Dockerfile`,
  `docker/Dockerfile.sync` and `docker/console/Dockerfile`, and the pin matrix
  records it. `rustls-webpki` 0.103.16 and `webpki-root-certs` 1.0.10 were
  already in the lock file.
