---
name: phase-done
description: Closes a tracker issue by verifying the work is genuinely done (acceptance criteria, gates, docs), writing the close narrative into the PR description, ensuring the PR declares `Closes #N`, and posting the handoff comment. Use when the user says a work item is complete or asks to close it out.
allowed-tools: Read, Edit, Grep, Glob, Bash
argument-hint: "[issue number] (optional)"
---

# /phase-done

The closing step of the issue workflow (`.claude/rules/issue-workflow.md`).
Only run this once the issue's work is actually finished; this skill verifies
and records, it does not decide the work is done on your behalf.

## Steps

1. **Identify the issue being closed** (the user names it, or it is the issue
   this branch's PR declares `Closes #N` for). Read it with
   `gh issue view <n> --json title,body,comments --jq '.title, .body,
   (.comments[] | "--- comment ---", .body)'` (`--comments` prints nothing
   for an issue without comments).
2. **Verify every `## Acceptance criteria` checkbox is ticked.** If any remain
   `- [ ]`, stop and list them; do not tick a criterion yourself just to
   proceed; a tick must reflect real, verified state (e.g. "workspace builds"
   means someone actually ran `cargo build --workspace` and it succeeded).
   Tick verified boxes in the issue body via `gh issue edit <n> --body-file`.
3. **Relationships check** (`scripts/gh/rel.sh tree <n>`;
   `.claude/rules/issue-relationships.md`): if the issue is a **parent** with
   **open sub-issues**, do NOT close it: finish or re-parent the children
   first. Closing this issue auto-unblocks anything it was `blocking`, which is
   expected; note any dependents that become workable so the handoff comment
   can point at them.
4. **Spec-adherence check:** for work that shipped spec-facing behaviour
   (a terminology operation, ECL, RF2 handling, subsumption), confirm it was
   checked against the FHIR/SNOMED/ECL specs (`/spec-lookup`) and, where
   applicable, against the Snowstorm/Hermes oracle. If that never happened,
   stop and say so; it is an unmet exit criterion in spirit.
5. **FHIR model pin:** if the work took a new `fhir-types` release, confirm
   the root requirement, `docs/VERSIONS.md`, and `Cargo.lock` all name it
   (`scripts/checks/versions.sh`).
6. **Changelog check:** a change with user-visible effect adds a fragment,
   `changelog.d/<issue>-<kebab-slug>.<section>.md` (`changelog.d/README.md`),
   and `scripts/release/changelog.sh --check` passes. Add the fragment if it is
   missing, and never edit `CHANGELOG.md` for it. A change with no
   user-visible effect carries the `no-changelog` label.
7. **Write the close narrative into the PR description:** what shipped, the key
   decisions with their spec citations, the gate results, and what was
   deliberately left out (with follow-up issue numbers). The PR description +
   the issue thread ARE the build record.
8. **Post the handoff comment on the issue** (`gh issue comment <n>`): where
   things stand at close, what was deliberately left out (with follow-up issue
   numbers), and what a follow-up session should do first.
9. **Ensure the PR body declares `Closes #<n>`** (`gh pr view` / `gh pr edit`)
   so the merge into `main` auto-closes the issue; never close the issue by
   hand when a PR carries the work. One `Closes` keyword per issue. Confirm
   auto-merge is armed (`gh pr view --json autoMergeRequest`); arm it if not,
   as its own command (`gh pr merge <n> --auto`, which adds it to the merge
   queue).
10. **Roadmap-board check** (`.claude/rules/project-board.md`): `Done` is set by
   the built-in workflow when the merge closes the issue, never by hand. After
   the merge, `scripts/gh/project.sh show <n>` should say `Done`; if the issue
   is missing from the board entirely, `scripts/gh/project.sh add <n>` and let
   the closed→Done workflow settle it. Do not archive or delete board items.
11. **Remind the user to commit** the close on the current conventional-type
   branch (`feat/…` etc.).

## What this skill does not do

It does not run `cargo build` or the test suite to "check" the acceptance
criteria for you; those must already have been run and have genuinely passed
before this skill is invoked. If in doubt, run the relevant `cargo` command
first.
