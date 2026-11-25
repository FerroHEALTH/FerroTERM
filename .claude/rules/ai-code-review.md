<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# Machine review (SonarQube Cloud): advisory, never authority

Every push to `main`, and a weekly schedule, is analyzed by SonarQube Cloud
(`.github/workflows/sonar.yml`; scope in `sonar-project.properties`; project
`FerroHEALTH_FerroTERM`, organization `ferrohealth`). Rust is analyzed
first-party (the analyzer runs Clippy over the workspace); shell, YAML, and
other languages are covered by the multi-language sweep. It exists as a second
opinion beside the local gates and CodeQL.

After each analysis of `main`, the workflow converts the open issues to SARIF
(`scripts/sonar/sarif.sh`) and uploads them to GitHub code scanning under the
category `sonarqube-cloud`, so SonarQube Cloud is listed as a code-scanning
tool beside CodeQL and Scorecard. Sonar's own code-scanning integration needs
its Enterprise plan. Those alerts are as advisory as the dashboard.

It is a **second opinion**. It is not authority, and it gates no merge.
Because it gates no merge, it runs on no pull request and no merge group
(owner decision, 2026-11-25): the organisation shares 20 concurrent jobs,
and an advisory analysis must not hold a runner a gate needs. A newer push
to `main` cancels a running analysis. CodeQL, the security scanner, keeps
its pull-request run, limited to changes under Rust sources, Cargo files or
`.github/`, and runs on no merge group.

## Precedence: a finding never outranks the sources

1. The FHIR + SNOMED/ECL specifications (`spec-adherence.md`): the oracle.
2. The hard rules: `CLAUDE.md`, the `.claude/rules/*.md`.
3. The local gates: `cargo fmt`, `clippy`, `cargo nextest`, `cargo deny`, the CI
   guards.
4. The analyzer.

A finding that contradicts a spec citation, or asks for something the rules
forbid, is wrong by construction. Nothing it reports relaxes `testing.md`: never
weaken a test because a finding suggested it.

## Rules

- **It gates no merge.** The quality gate is informational; the README badge
  reflects it, but merges are decided by the local gates + review.
- **Findings are acted on by hand, in a normal change**, never applied through
  a UI that would attribute a commit to a bot (the no-AI-attribution rule has no
  exceptions).
- **Automatic Analysis stays OFF** (owner setting): CI-based analysis and
  SonarCloud Automatic Analysis cannot both run. `sonar.yml` is the analysis
  path.
- **A wrong finding is data, not a silent suppression:** record it (a tracker
  issue or a scope adjustment in `sonar-project.properties`), and only change
  scope when the scope is genuinely wrong.

CodeQL (security) runs separately (`.github/workflows/codeql.yml`) and is also
advisory here until a precision case is made to gate on it.

## The third lane: GitHub's Copilot code scanning ("Code scanning AI findings")

A check named "Code scanning AI findings" (`github-advanced-security`) runs
on pull requests. It is GitHub's Copilot code-scanning service, enabled in the
repository's Code security settings, not a workflow in this tree. It is
advisory like the other two lanes and gates no merge. Since 2026-09-22 it
fails on every pull request with a service error from GitHub's side (`CAPIError:
400 The requested model is not supported`) and reports no alert; a failure
with that text is noise, not a finding. The owner decides in the repository
settings whether the lane stays; the workflows and guards here are unchanged
either way.
