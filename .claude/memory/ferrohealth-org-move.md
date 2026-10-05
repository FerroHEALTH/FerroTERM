---
name: ferrohealth-org-move
description: "FerroTERM moved from rubentalstra/FerroTERM to FerroHEALTH/FerroTERM on 2026-10-01; what moved with it, what did not, and the owner steps left"
metadata:
  node_type: memory
  type: project
  originSessionId: f27ec2c3-bb6e-4c42-8473-9aa886840aef
  modified: 2026-10-01T15:55:50.606Z
---

On 2026-10-01 the owner asked to transfer the repository to the FerroHEALTH organisation and fix the configuration; done via `gh api repos/rubentalstra/FerroTERM/transfer` (PR #698 rewrote the tree, v0.1.6 is the first release from the org).

- Moved with the repo: issues, PRs, releases, secrets (SONAR_TOKEN), environments (crates-io, github-pages), rulesets, private vulnerability reporting.
- Reset by the transfer: homepage (set back to https://ferroterm.eu/), secret scanning + push protection (re-enabled), Pages custom domain (set to ferroterm.eu; www CNAMEs to FerroHEALTH.github.io).
- Did NOT move: attestations stay under `users/rubentalstra`, so v0.1.5 and earlier verify with `--owner rubentalstra --signer-workflow rubentalstra/FerroTERM/...`; the GHCR package stays at `ghcr.io/rubentalstra/ferroterm` (copying tags to `ghcr.io/ferrohealth/ferroterm` with crane was refused by the permission classifier, left to the owner).
- The image path is hardcoded lowercase `ghcr.io/ferrohealth/ferroterm`: `${{ github.repository_owner }}` is `FerroHEALTH`, invalid in an image name.
- crates.io Trusted Publishing checks the owner id, so every crate needs its GitHub entries re-added with owner `FerroHEALTH` (owner step) before a tag publishes crates.
- The org has three owners with admin on the repo (rubentalstra, AlessandroTorrisi, sebastian-iancu); MAINTAINERS.md says so.
- The org is on the free plan: CI queues behind FerroEHR and the other org repos' jobs.
- Sonar moved on 2026-10-05 (#704): the project is `FerroHEALTH_FerroTERM` in the SonarQube Cloud organization `ferrohealth`, as FerroFED and FerroEHR; the old `rubentalstra_FerroTERM` project is retired, and main's open issues reach code scanning as SARIF (category `sonarqube-cloud`).
- FerroBRIDGE still lives at rubentalstra/FerroBRIDGE.

Related: [[release-cut-cadence]], [[repo-merge-gates]], [[container-image-decisions]].
