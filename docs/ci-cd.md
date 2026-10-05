# CI/CD and supply-chain

No spec governs this: our own design, grounded in SLSA v1.0, OpenSSF, Sigstore,
and the OWASP GitHub Actions Security Cheat Sheet. The enforceable discipline is
`.claude/rules/ci-cd.md`; this document is the design and the rationale.

The foundation is stood up before product code so it is not an afterthought. It
is built to stay green on a pre-code repo: config/workflow/shell analysis runs
now; Rust and release lanes activate automatically when the workspace and tags
exist.

## The workflows

| Workflow | Does | Active |
|---|---|---|
| `ci.yml` (workflows lane) | `actionlint`, `zizmor --min-severity=low` over `.github/workflows/`, `shellcheck` over tracked shell | now |
| `ci.yml` (Rust lanes) | fmt, clippy `-D warnings`, nextest `--locked` + doctests, rustdoc `-D warnings`, `cargo deny`, MSRV (`cargo hack check --rust-version`), dependency-review (PR) | on workspace |
| `ci.yml` (`versions` job) | `scripts/checks/versions.sh`, version-drift guard | now |
| `scorecard.yml` | OpenSSF Scorecard, results published, SARIF → code scanning | now |
| `codeql.yml` | CodeQL: `actions` now, `rust` self-activates when the workspace lands | actions now |
| `sonar.yml` | SonarQube Cloud (advisory; `.claude/rules/ai-code-review.md`) | now (sweep); coverage on workspace |
| `release-build.yml` | reusable SLSA Build L3 lane: build + SBOM + keyless attest | on tag |
| `release.yml` | `v*` orchestrator: draft → per-arch build → image → verify assets → publish last | on tag |
| `release-image.yml` | reusable SLSA Build L3 lane: the distroless static image from the attested musl binaries, pushed to GHCR with provenance and SBOM attestations on the index and on each platform manifest | on tag |
| `ci.yml` (`hadolint` job) | `hadolint` over every `Dockerfile` in the tree, plus `docker/Dockerfile.sync` by name | now |
| `ci.yml` (`viewer` job) | the viewer's WebAssembly lane, run over both bundles the crate builds: `cargo fmt` and `leptosfmt --check` over `app/ferroterm-viewer`, `cargo clippy --target wasm32-unknown-unknown -D warnings` with and without `--features editor`, `cargo nextest run -p ferroterm-viewer` in both, `trunk build --release --locked` for each, and the recorded bundle size of each (`scripts/checks/bundle-size.sh`) | on workspace |
| `ci.yml` (`viewer-boundary` job) | the viewer's resolved dependency closure links no workspace crate (`scripts/checks/viewer-boundary.sh`) | on workspace |
| `ci.yml` (`no-client-in-server` job) | the server binary's resolved dependency tree carries no HTTP client, so its only outbound call is the OIDC issuer the optional SMART gate reads (`scripts/checks/no-client-in-server.sh`) | on workspace |
| `ci.yml` (`addon-boundary` job) | only `app/ferroterm-sync` links an `addons/*` crate, and an add-on links the syndication client and the leaf crates the script names (`scripts/checks/addon-boundary.sh`, run with `--self-test` first) | on workspace |
| `ci.yml` (`tree-guards` job) | every `scripts/vendor/*.sh` fetcher writes into the tree (`vendor-dests.sh`), every workspace member is named in the repo map and the architecture table (`repo-map.sh`), and every evidence citation in `docs/claims.md` resolves to a symbol or a test and names no line number (`claims.sh`), and every `https://ferroterm.eu/fhir/StructureDefinition/` canonical the Rust sources emit has its definition under `website/landing/fhir/StructureDefinition` (`extension-definitions.sh`) | always |
| `ci.yml` (`comment-style` job) | the RFC 505 / RFC 1574 comment budgets over every hand-written `.rs` file (`scripts/checks/comment-style.sh --all`), the sweep the per-edit hook cannot give | now |
| `ci.yml` (`bench-bars` job) | the four operations timed over a generated edition, each median compared to the claim in `bench/bars.json` (`scripts/checks/bench-bars.sh`) | on workspace |
| `fuzz.yml` | weekly: every parser a client or a release reaches, fed arbitrary bytes under `cargo-fuzz` on nightly (`fuzz/README.md`) | weekly |
| `ci.yml` (`tx-ecosystem` job) | the HL7 terminology ecosystem suite (`general` mode) against a release build through the FHIR Validator's `txTests`, gated by the committed pass list `conformance/tx-ecosystem/passing.txt` (`scripts/checks/tx-ecosystem.sh`) | on workspace |
| `ci.yml` (`ui-e2e` job) | the viewer's browser journeys: the Trunk bundle embedded in the server, the image built from `docker/Dockerfile`, and a digest-pinned headless Chromium driving it over WebDriver (`scripts/ui-e2e.sh`) | on workspace |
| `differential.yml` | weekly: the request sample of `conformance/differential/` answered by FerroTERM and by Snowstorm over the same licensed edition (`scripts/checks/differential.sh`), an issue filed on divergence | when `SNOMED_RF2_URL` and `SNOWSTORM_URL` are set |

Dependabot (`github-actions` + `cargo`) keeps `uses:` SHAs (the workflows and
the composite action under `.github/actions`) and crate pins current.

## SLSA Build Level 3

L3 adds two guarantees over L2: builds are isolated from each other, and the
signing identity is not reachable by user build steps ("provenance is
unforgeable"). On GitHub-hosted runners this is achieved by putting the build
**and** its attestation in a **reusable workflow** (`release-build.yml`,
`on: workflow_call`). It runs on its own VM the caller cannot inject steps into,
so the Sigstore/Fulcio identity that signs names *that workflow*. Do not inline
the build+attest into a normal job; that is only L2.

Consumers verify provenance against the signer workflow:

```
gh attestation verify ferroterm-<tag>-<target>.tar.gz -R FerroHEALTH/FerroTERM \
  --signer-workflow FerroHEALTH/FerroTERM/.github/workflows/release-build.yml
```

We claim L3 by GitHub's builder isolation; we do not (yet) claim reproducible or
hermetic builds; those are separate SLSA tracks.

## SBOM and signing

Each release artifact carries: an embedded dependency list (`cargo auditable`
writes it into the binary's `.dep-v0` section), a CycloneDX SBOM
(`cargo cyclonedx`), and a `.sha256`. The provenance and the SBOM are signed
**keyless** via Sigstore (`actions/attest-build-provenance` and
`actions/attest-sbom`, `id-token: write`), so there is no long-lived key. A
"signed SBOM" here is the SBOM wrapped in a Sigstore DSSE bundle bound to the
artifact digest and signed by the pinnable workflow identity, verifiable with
`gh attestation verify`.

## The viewer bundle

`app/ferroterm-viewer` compiles to WebAssembly and reaches a user as a bundle
inside the `ferroterm` binary, so it has no image, no tarball, and no
attestation subject of its own.

On a pull request the `viewer` job is the static gate. It formats the crate
(`cargo fmt` plus `leptosfmt --check`, which reads the `view!` macros rustfmt
leaves alone), runs clippy against `wasm32-unknown-unknown`, runs the crate's
tests, builds the release bundle with Trunk, and measures it.
`scripts/checks/bundle-size.sh` gzips each emitted asset and compares it to the
claim in `app/ferroterm-viewer/bundle-size.json`, the shape `bench-bars.sh`
uses for latency: the bar is what the project claims, and a breach is bytes to
justify or a claim to withdraw. The clippy pass carries the most weight, since
the browser target is the only place a dependency that cannot compile for
WebAssembly shows up. Trunk 0.21.14 and `leptosfmt` 0.1.33 are installed at the
versions `docs/VERSIONS.md` pins, and Trunk downloads the Tailwind standalone
CLI itself at the version `Trunk.toml` names, so there is no Node in the lane.
`scripts/checks/versions.sh` fails when any of those three pins drift apart.

None of that proves the bundle renders, because the viewer is client-side
rendered and the document is an empty `<body>` until the WebAssembly module
boots. The `ui-e2e` job is where that is proved. `scripts/ui-e2e.sh` builds the
bundle, embeds it in a server binary, builds the image from
`docker/Dockerfile` the way the release lane stages it, and runs that image
beside a headless Chromium pinned by index digest, both on a private container
network. The journeys are plain `#[tokio::test]`s in `e2e/`, a crate the root
manifest excludes: `thirtyfour` depends on `serde_json` with `preserve_order`,
and cargo unifies features across one invocation, so holding it in the
workspace would make the test run write FHIR JSON in a key order the shipped
server does not. The job therefore also runs the formatting and clippy passes
that crate would otherwise miss.

A sign-in cannot be driven against that one deployment, because a server either
names an identity provider or does not and the read-only journeys assert the
second shape. The script starts a second server beside it, a stub identity
provider from `e2e/src/bin/stub_issuer.rs`, and a TLS terminator in front of
the pair: the server refuses a non-loopback issuer over plain HTTP, and a
browser gives `crypto.subtle` to a secure context alone, so both legs are
`https`. The authority is generated per run and never committed. The server
container takes it through `SSL_CERT_FILE`, which is how
`rustls-native-certs` is told where its roots are, and the browser takes it
through Chromium's `CACertificates` enterprise policy, so no certificate check
is turned off anywhere.

A failing journey leaves the page it failed on. The harness writes a
screenshot and the whole document into `target/ui-e2e-failures`, and the job
uploads that directory as the `ui-e2e-failure` artifact when the run fails, so
a failure that reproduces only on the runner is looked at rather than re-run. A
passing run writes nothing there, and the capture itself never raises: a
screenshot the browser cannot answer is reported in the failure message. The
browser container is started with `SE_NODE_MAX_SESSIONS` raised to the runner's
processor count and the journeys run with the same test-thread count, so they
run beside each other rather than queueing on the grid's one default session.

On a tag, `release-build.yml` runs `trunk build --release --locked` twice
before `cargo auditable build`, once for the reader bundle and once with
`--features editor` for the editor bundle, and builds the server with the
feature that embeds both directories into the binary. The bundle is architecture-independent, so the build
is the same work in each per-architecture job; it happens inside the reusable
lane because a bundle handed in by a caller job would put bytes the L3 builder
did not produce inside the artifact it signs. The release therefore gains no
new asset, no new attestation subject, and no change to the SLSA shape, and
`docker/Dockerfile` gains no stage: the image copies the same two binaries it
always did, one of which now carries the web UI. The feature name lives in one
place, the `VIEWER_FEATURE` variable at the top of `release-build.yml`, and
`scripts/checks/versions.sh` fails when the server stops declaring it. The lane
also sets `FERROTERM_UI_BUNDLE` and `FERROTERM_UI_EDITOR_BUNDLE` to the
directories Trunk just wrote, because the server's build script refuses a named
directory that does not read and only warns when it falls back to the default.
A release that lost a bundle therefore fails to build rather than shipping a
binary with no viewer in it.

## The container image

`ghcr.io/ferrohealth/ferroterm` (`linux/amd64` and `linux/arm64`) is
`docker/Dockerfile`: the static musl binary copied root-owned onto
`gcr.io/distroless/static-debian13:nonroot`, pinned by index digest and bumped
by Dependabot. Distroless static brings `/etc/passwd`, `/tmp`, tzdata, and
ca-certificates for about 2 MiB and is itself keyless-signed; there is no shell
and no package manager. The user is the numeric `65532:65532` (the kubelet
refuses `runAsNonRoot` on a named user), the entrypoint is exec-form so the
binary is PID 1 and receives `SIGTERM` itself, and the `HEALTHCHECK` is the
binary probing its own `GET /health` (`ferroterm healthcheck`), since the base
has no shell or HTTP client and Kubernetes ignores the instruction anyway. The
root `.dockerignore`
denies everything but the staged `dist/` tree, so no source, vendored package,
or build output enters the context.

The image is built in its own reusable lane (`release-image.yml`) for the same
L3 reason as the binaries: the signing identity names that workflow and no
caller step can reach it. The lane downloads the two musl tarballs the binary
lane built in the same run, verifies each against `release-build.yml`'s identity
before extracting the binary, builds both platforms with no cache and BuildKit
`provenance: mode=max`, then attests with `actions/attest` and
`push-to-registry`: SLSA provenance for the index and for each platform manifest,
and one syft SPDX SBOM per platform manifest (syft reads the `cargo auditable`
dependency list inside the binary, so the SBOM names the crates). BuildKit's own
SBOM scanner is off; it would see a two-file filesystem. The lane finishes by
verifying its own output as a consumer would, and the release does not publish
without it.

Tags are `<version>`, `<major.minor>`, and `latest` (skipped for a
pre-release); GHCR tags are mutable, so deploy by digest. Verify:

```
gh attestation verify oci://ghcr.io/ferrohealth/ferroterm:<version> \
  -R FerroHEALTH/FerroTERM \
  --signer-workflow FerroHEALTH/FerroTERM/.github/workflows/release-image.yml
gh attestation verify oci://ghcr.io/ferrohealth/ferroterm@<platform digest> \
  -R FerroHEALTH/FerroTERM --predicate-type https://spdx.dev/Document/v2.3
```

## OpenSSF

- **Scorecard** runs weekly and on push/branch-protection changes, publishing
  results. We earn the checks honestly: pinned dependencies (SHA-pinned `uses:` +
  Dependabot digest bumps), minimal token permissions, no dangerous-workflow
  patterns, branch protection, CodeQL (SAST + code scanning), Dependabot
  (dependency-update-tool), signed releases, a security policy. Two checks score
  low for a new solo repo by construction: **Maintained** (needs 90-day
  activity) and **Code-Review** (needs a second human); these are accepted, not
  chased.
- **Best Practices badge:** register at bestpractices.dev and fill the passing
  self-assessment (version control, unique versioning, issue tracker, license,
  basic docs, HTTPS, no known-unpatched vulns are already satisfiable). Silver
  and gold follow as the project matures; SLSA provenance + signed releases
  materially help the higher tiers.

## Rust supply chain

`cargo deny check` (advisories/licenses/bans/sources, subsumes cargo-audit),
`Cargo.lock` committed with every CI build `--locked`, MSRV verified with
`cargo hack check --rust-version`, and `dependency-review-action` blocking
vulnerable or license-incompatible new deps on PRs. `cargo-vet` and a `cargo-fuzz`
harness are maturity additions.

## GitHub hardening

Every `uses:` SHA-pinned; `permissions: {}` + least-privilege per job;
`persist-credentials: false`; no context interpolation in `run:`; no build cache
in publishing lanes. A `main` ruleset requires PRs, the `conclusion` status
check, and signed commits, blocks force-push and deletion, and merges through
GitHub's merge queue (squash, all-green grouping, up to 5 entries built at a
time). Releases are immutable (freeze on publish; recover with a new patch
version, never a retag).

## The merge queue

`main` takes every pull request through the merge queue
(<https://docs.github.com/en/repositories/configuring-branches-and-merges-in-your-repository/configuring-pull-request-merges/managing-a-merge-queue>).
A pull request is armed with `gh pr merge <n> --auto`, which adds it to the
queue once `conclusion` is green; the queue sets the merge method itself and
refuses `--squash` and `--delete-branch`, and the repository deletes a merged
head branch on its own. The queue builds each entry on top of the entries
ahead of it, so the strict up-to-date policy is off and no pull request needs
a rebase just because another one merged first.

`ci.yml` runs on `merge_group`, and the `conclusion` job reports on the queued
commit. The jobs that read a pull-request payload (`contribution-licence-guard`,
`crate-version-guard`, `changelog-guard`, `dependency-review`) skip on a merge
group: each pull request passed them on its own before it joined the queue, and
`conclusion` counts a skip as a pass. The pull-request trigger also listens for
`labeled` and `unlabeled`, so adding `no-crate-bump` or `no-changelog` re-runs
CI without a new push.

## Owner actions (one-time, cannot be scripted)

- `main` ruleset: require PR + the `conclusion` status check, **require signed
  commits**, block force-push + deletion, merge queue on (squash, `ALLGREEN`).
- Enable: code scanning, secret scanning + push protection, Dependabot
  alerts/updates, artifact attestations. (Immutable releases: already enabled.)
- Add the `SONAR_TOKEN` repository secret. (Done.) Keep SonarCloud **Automatic
  Analysis OFF** (done) so CI-based analysis is authoritative.
- Register the project at bestpractices.dev; add the returned badge to the
  README.
- Run `scripts/gh/migrate-fields.sh apply` and `verify`, then
  `scripts/gh/labels.sh`, to move the tracker onto the organisation's issue
  types and fields and converge the label taxonomy.
- The "FerroTERM Roadmap" organisation project: the built-in workflows
  (auto-add, closed to Done, reopened to Todo) are set in the project UI.

## Sources

SLSA v1.0 <https://slsa.dev/spec/v1.0/levels> · GitHub artifact attestations
<https://docs.github.com/actions/security-guides/using-artifact-attestations-and-reusable-workflows-to-achieve-slsa-v1-build-level-3>
· SBOM CycloneDX <https://cyclonedx.org/> · `cargo-auditable`
<https://github.com/rust-secure-code/cargo-auditable> · OpenSSF Scorecard
<https://securityscorecards.dev/> · Best Practices <https://www.bestpractices.dev/>
· Sigstore cosign <https://docs.sigstore.dev/cosign/> · cargo-deny
<https://embarkstudios.github.io/cargo-deny/> · OWASP GHA Cheat Sheet
<https://cheatsheetseries.owasp.org/cheatsheets/GitHub_Actions_Security_Cheat_Sheet.html>
· zizmor <https://docs.zizmor.sh/>
