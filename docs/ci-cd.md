<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

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
| `ci.yml` (Rust lanes) | fmt, clippy `-D warnings`, nextest `--locked` + doctests, rustdoc `-D warnings`, `cargo deny` with no stale advisory exception (`advisory-exceptions.sh`) and every VEX statement's dependency path against the graph (`vex-reachability.sh`), MSRV (`cargo hack check --rust-version`), dependency-review (PR) | on workspace |
| `ci.yml` (`versions` job) | `scripts/checks/versions.sh`, version-drift guard | now |
| `pr-guards.yml` | on a label change, re-runs the guard it affects (`changelog-guard`, `crate-version-guard`) in the head commit's CI run once that run completes | now |
| `scorecard.yml` | OpenSSF Scorecard, results published, SARIF → code scanning | now |
| `codeql.yml` | CodeQL: `actions` now, `rust` self-activates when the workspace lands | actions now |
| `sonar.yml` | SonarQube Cloud (advisory; `.claude/rules/ai-code-review.md`) | now (sweep); coverage of the workspace tests and of the console under the browser journeys (`scripts/ui-e2e-coverage.sh`) |
| `release-build.yml` | reusable SLSA Build L3 lane: build + SBOM + keyless attest | on tag |
| `release.yml` | `v*` orchestrator: plan → draft → per-arch build → images by digest → vulnerability record → scan and tag → verify assets → publish last | on tag |
| `release-image.yml` | reusable SLSA Build L3 lane: the distroless static image from the attested musl binaries, pushed to GHCR by digest with provenance and SBOM attestations on the index and on each platform manifest | on tag |
| `scan-and-tag.yml` | reusable: both pushed digests scanned with Trivy (HIGH/CRITICAL with a fix, `trivy.yaml`), and only then tagged | on tag |
| `image-scan.yml` | weekly: the published `latest` images scanned with every `security/vex/` document as `--vex`, one tracking issue filed or updated on a finding | weekly |
| `ci.yml` (`vex` job) | the Rust-advisory OpenVEX document regenerated from `deny.toml` and `security/vex/rust-advisories.toml` and diffed, every hand-written document well-formed (`scripts/checks/vex-advisories.sh`), and the release record's coverage check self-tested | always |
| `ci.yml` (`hadolint` job) | `hadolint` over every `Dockerfile` in the tree, plus `docker/Dockerfile.sync` by name | now |
| `ci.yml` (`console-boundary` job) | the console's resolved dependency closure links no workspace crate (`scripts/checks/console-boundary.sh`) | on workspace |
| `ci.yml` (`no-client-in-server` job) | the server binary's resolved dependency tree carries no HTTP client outside `ferrohealth-smart` and `ferrohealth-usage-report`, so its only outbound calls are the authorization server `[auth]` names and the usage report (`scripts/checks/no-client-in-server.sh`) | on workspace |
| `ci.yml` (`addon-boundary` job) | only `app/ferroterm-sync` links an `addons/*` crate, and an add-on links the syndication client and the leaf crates the script names (`scripts/checks/addon-boundary.sh`, run with `--self-test` first) | on workspace |
| `ci.yml` (`tree-guards` job) | every `scripts/vendor/*.sh` fetcher writes into the tree (`vendor-dests.sh`), every workspace member is named in the repo map and the architecture table (`repo-map.sh`), and every evidence citation in `docs/claims.md` resolves to a symbol or a test and names no line number (`claims.sh`), and every `https://ferroterm.eu/fhir/StructureDefinition/` canonical the Rust sources emit has its definition under `website/landing/fhir/StructureDefinition` (`extension-definitions.sh`) | always |
| `ci.yml` (`comment-style` job) | the RFC 505 / RFC 1574 comment budgets over every hand-written `.rs` file (`scripts/checks/comment-style.sh --all`), the sweep the per-edit hook cannot give | now |
| `ci.yml` (`bench-bars` job) | the four operations timed over a generated edition, each median compared to the claim in `bench/bars.json` (`scripts/checks/bench-bars.sh`) | on workspace |
| `fuzz.yml` | weekly: every parser a client or a release reaches, fed arbitrary bytes under `cargo-fuzz` on nightly (`fuzz/README.md`) | weekly |
| `ci.yml` (`tx-ecosystem` job) | the HL7 terminology ecosystem suite (`general` mode) against a release build through the FHIR Validator's `txTests`, gated by the committed pass list `conformance/tx-ecosystem/passing.txt` (`scripts/checks/tx-ecosystem.sh`) | on workspace |
| `ci.yml` (`ui-e2e` job) | the console's browser journeys: the server, the console and the stub identity provider on loopback, driven by Chrome for Testing at the pinned release over WebDriver (`scripts/ui-e2e.sh`) | on workspace |
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

## The console

`app/ferroterm-console` is the FerroTERM Console, its own binary and image
(`ghcr.io/ferrohealth/ferroterm-console`), every page drawn on its server
with FerroUI 0.6. It is one crate built by `cargo build`: there is no
WebAssembly half, no site bundle and no Node in any lane, and the workspace
lanes build, lint and test it like any other crate. `scripts/checks/versions.sh`
fails when the Chrome for Testing pin of `docs/VERSIONS.md` and a workflow
drift apart.

The `ui-e2e` job proves what a reader sees. `scripts/ui-e2e.sh` builds the
server, the console (in release, the way the release lane builds it) and the stub identity
provider from `e2e/src/bin/stub_issuer.rs`, starts two servers, the provider
and two consoles on loopback, one console offering no sign-in and one signing
in at the provider, and runs the journeys against them with Chrome for Testing
and its chromedriver at the release `docs/VERSIONS.md` pins. On a developer
machine without them the script fetches that release into
`target/journey-tools` with `scripts/release/journey-tools.sh`, checked
against its pinned SHA-256. Every address is loopback, so no certificate is needed and no check
is turned off. The journeys
are plain `#[tokio::test]`s in `e2e/`, a crate the root manifest excludes:
`thirtyfour` depends on `serde_json` with `preserve_order`, and cargo unifies
features across one invocation, so holding it in the workspace would make the
test run write FHIR JSON in a key order the shipped server does not. The job
therefore also runs the formatting and clippy passes that crate would
otherwise miss.

The journeys run with FerroUI's script modules loaded, and the `no_script`
journeys run the same pages with every module refused through the
DevTools protocol, so a page that needs a script to work fails the merge.
The `reflow` journeys draw every screen at 320 CSS pixels and fail on a
sideways scroll (WCAG 2.2 SC 1.4.10).

A failing journey leaves the page it failed on: the harness writes a
screenshot and the whole document into `target/ui-e2e-failures`, and the job
uploads that directory as the `ui-e2e-failure` artifact.

On a tag, `release-console.yml` (called by `release.yml`) builds the console's
binary per architecture, attests it, and publishes
the multi-platform image from `docker/console/Dockerfile`, which copies what
the lane staged and compiles nothing. The terminology server's image carries
no user interface.

## The container image

`ghcr.io/ferrohealth/ferroterm` (`linux/amd64` and `linux/arm64`) is
`docker/Dockerfile`: the static musl binary copied root-owned onto
`gcr.io/distroless/static-debian13:nonroot`, pinned by index digest and bumped
by Dependabot. Distroless static brings `/etc/passwd`, `/tmp`, tzdata, and
ca-certificates for about 2 MiB and is itself keyless-signed; there is no shell
and no package manager. The user is the numeric `65532:65532` (the kubelet
refuses `runAsNonRoot` on a named user), the entrypoint is exec-form so the
binary is PID 1 and receives `SIGTERM` itself, and the `HEALTHCHECK` is the
binary probing its own `GET /health/readiness` (`ferroterm healthcheck`), since the base
has no shell or HTTP client and Kubernetes ignores the instruction anyway. The
root `.dockerignore`
denies everything but the staged `dist/` tree, so no source, vendored package,
or build output enters the context.

The image is built in its own reusable lane (`release-image.yml`) for the same
L3 reason as the binaries: the signing identity names that workflow and no
caller step can reach it. The release calls the lane twice, once for the
server image (`docker/Dockerfile`, `ferroterm` and `ferroterm-build`) and once
for the sync image `ghcr.io/ferrohealth/ferroterm-sync` (`docker/Dockerfile.sync`,
`ferroterm-sync` and `ferroterm-build`), both from the same musl tarballs. The lane downloads the two musl tarballs the binary
lane built in the same run, verifies each against `release-build.yml`'s identity
before extracting the binary, builds both platforms with no cache and BuildKit
`provenance: mode=max`, then attests with `actions/attest` and
`push-to-registry`: SLSA provenance for the index and for each platform manifest,
and one syft SPDX SBOM per platform manifest (syft reads the `cargo auditable`
dependency list inside the binary, so the SBOM names the crates). BuildKit's own
SBOM scanner is off; it would see a two-file filesystem. The lane finishes by
verifying its own output as a consumer would, and the release does not publish
without it.

The lane pushes the index by digest with no tag. The release's
`vulnerability-record` job then scans every image and the server's glibc
tarballs with every severity, unfixed findings included, and
`scripts/checks/vex-coverage.sh` refuses the release while a finding has no
judging statement under `security/vex/` (CRA Annex I Part I(2)(a));
the record, `ferroterm-<tag>-vulnerability-record.json`, and the raw Trivy
reports are release assets. `scan-and-tag.yml` then fails on any HIGH or
CRITICAL finding with a fix and only after that applies the tags, and the
release publishes only after both, so a refused release leaves no tag and no
published page. The weekly `image-scan.yml` scans the published images again
with the VEX documents applied (`security/vex/README.md`).

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
commit. The jobs that read a pull-request payload (`crate-version-guard`,
`changelog-guard`, `dependency-review`) skip on a merge
group: each pull request passed them on its own before it joined the queue, and
`conclusion` counts a skip as a pass.

The pull-request trigger lists `opened`, `synchronize` and `reopened` only, so
a commit gets one CI run: a label event would start a second run on the same
commit, the concurrency group would cancel the first, and its `conclusion`
would report a failure. The guards read their escape labels from the API when
they run. `pr-guards.yml` listens for `labeled` and `unlabeled`, waits for the CI run on the head commit to
complete, and re-runs the guard the change affects, which re-runs `conclusion`
with it (<https://docs.github.com/en/rest/actions/workflow-runs#re-run-a-job-from-a-workflow-run>).

## Owner actions (one-time, cannot be scripted)

- `main` ruleset: require PR + the `conclusion` status check, **require signed
  commits**, block force-push + deletion, merge queue on (squash, `ALLGREEN`).
- Enable: code scanning, secret scanning + push protection, Dependabot
  alerts/updates, artifact attestations. (Immutable releases: already enabled.)
- Add the `SONAR_TOKEN` repository secret, for the SonarQube Cloud project
  `FerroHEALTH_FerroTERM` in the organization `ferrohealth`. (Done.) Keep
  SonarCloud **Automatic Analysis OFF** (done) so CI-based analysis is
  authoritative.
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
