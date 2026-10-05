# Issue workflow (the tracker loop)

**The tracker is GitHub Issues: the open issue list IS the worklist.** Issue
state is edited only via `gh`; never track work only in chat. This file is the
loop, the label taxonomy, and the cadence. Relationships between issues live in
`.claude/rules/issue-relationships.md`; the public board is
`.claude/rules/project-board.md`.

## The loop

1. **Orient.** `gh issue list --state open` (the SessionStart hook injects it,
   annotated with each issue's sub-issue progress `{k/n}`, `child-of #parent`,
   and open `BLOCKED-by`/`blocks` edges). Pick the pinned issue (pins = current
   focus, max 3) or the issue the user names; **skip an issue shown
   `BLOCKED-by` an open issue** (work its blocker first) and prefer the next
   open child of a parent. Among the candidates, the `Priority` field decides
   the order (`Urgent`, `High`, `Medium`, `Low`) and the oldest issue goes
   first within a priority. Read the contract with the `--json` form below and
   its relationship graph with `scripts/gh/rel.sh tree <n>`.

   **Never read an issue with `gh issue view <n> --comments`.** On gh 2.101.0
   it prints nothing at all and exits 0 for an issue with no comments, so the
   contract comes back empty and nothing says so. Every reader of an issue
   uses:

   ```sh
   gh issue view <n> --json title,body,comments \
     --jq '.title, .body, (.comments[] | "--- comment ---", .body)'
   ```
2. **Read the contract.** The issue body opens with a plain summary (no
   heading: what + why, decisions, spec citations), followed by an
   `## Acceptance criteria` checklist (+ optional `## Tasks` task list). New
   work discovered en route gets its own issue (`scripts/gh/fields.sh new`,
   with a type, a priority and an effort) that is then **linked** with
   `scripts/gh/rel.sh`, a sub-issue of the issue it decomposes, or a
   `blocked-by`/`blocking` dependency for real sequencing, never a prose "see
   also" deferral.
3. **Do the work.** At pickup, move the issue to `In Progress` on the board
   (`scripts/gh/project.sh status <n> in-progress`). First read the governing
   spec text (`/spec-lookup`; the FHIR + SNOMED/ECL specs are the oracle, see
   `.claude/rules/spec-adherence.md`). For the FHIR wire layer, change the
   generator and regenerate (never hand-edit `// @generated`); for the SNOMED
   engine, build idiomatic Rust of our own design, the specs as the authority.
   Build compiling, tested increments.
4. **Record progress on the issue.** Tick verified acceptance-criteria
   checkboxes (`gh issue edit <n>`), post substantive status/decisions as
   comments (`gh issue comment <n>`); the issue thread is the durable record.
5. **Commit on a conventional-type branch** (see the branch rule below) with a
   descriptive subject; the PR body declares `Closes #<n>` so the merge into
   `main` auto-closes the issue (never close by hand when a PR carries the
   work). One `Closes` keyword closes one issue: "Closes #1, #2" closes only
   #1; repeat the keyword per issue and verify after merge. Arm auto-merge the
   moment the PR is open, as its own command: `gh pr merge <n> --auto`, which
   adds it to the merge queue (`.claude/memory/auto-merge-every-pr.md`).
6. **Close out** with `/phase-done`: verify the acceptance criteria are met,
   write the close narrative into the PR description, post the handoff comment
   on the issue.

## Type, priority and labels

**The type, the priority and the effort of an issue are GitHub's own fields,
never labels** (owner decision 2026-10-05, the model FerroFED has run since
2026-10-02). The `FerroHEALTH` organisation defines all three, and
`scripts/gh/fields.sh` sets each by issue number, resolving every node id and
failing loud on a typo:

| Fact | Where it lives | Values | Command |
|---|---|---|---|
| Type | the native issue type of the organisation | `Bug`, `Feature`, `Task` | `scripts/gh/fields.sh type <n> bug\|feature\|task` |
| Priority | the organisation's `Priority` issue field | `Urgent`, `High`, `Medium`, `Low` | `scripts/gh/fields.sh priority <n> urgent\|high\|medium\|low` |
| Effort | the organisation's `Effort` issue field | `High`, `Medium`, `Low` | `scripts/gh/fields.sh effort <n> high\|medium\|low` |

`scripts/gh/fields.sh show <n>` prints all three with the labels and the
milestone. `scripts/gh/fields.sh new <type> <priority> <effort> <gh issue
create args…>` creates an issue that carries all three from the start, so
every issue is filed through it, never through a bare `gh issue create`. The
SessionStart dump and `/phase-status` print type and priority as
`<Type/Priority>` after the title.

**An issue a scheduled lane files arrives without the three, and whoever
picks it up sets them.** `differential.yml` runs with the workflow's default
token, which may not read the organisation's issue types and issue fields:
GraphQL answers `organization: null` rather than an error. `fields.sh new`
probes for that, says so on stderr, and files the issue with the labels it was
given, so the finding is never lost. Whoever picks such an issue up sets the
type, the priority, the effort and the milestone before doing anything else
with it.

The priorities read: `Urgent` is drop everything, `High` is the current focus,
`Medium` is normal, `Low` is the backlog. Effort is the size of the work as
filed, in the filer's judgement: `Low` is one sitting (a comment, a guard, a
one-file fix), `Medium` is one pull request that touches more than one crate
or needs a test fixture, `High` is more than one pull request or a design the
orchestrator has to hold in context. It is set at filing and re-set when the
work turns out bigger; it never changes the order of the worklist, which the
priority and the age of the issue decide.

The type maps to the conventional-commit type of the branch and the commit: a
`Bug` is a `fix`, a `Feature` is a `feat`, and a `Task` says which one it is
with exactly ONE work-kind label. `scripts/gh/migrate-fields.sh` moves every
issue, open and closed, off the labels that carried these before (`bug` to
Bug, `enhancement` to Feature, everything else to Task; `P0` to Urgent, `P1`
to High, `P2` to Medium, `P3` to Low), gives every open issue its judged
effort and every Task its work-kind label. It is re-runnable (`plan`,
`apply`, `verify`), and it runs before `scripts/gh/labels.sh` deletes the six
labels, which a re-run of `labels.sh` retires again wherever they reappear.

Labels carry what the platform has no field for. Bootstrap them with
`scripts/gh/labels.sh`:

- **Work kind, on a `Task` only, exactly ONE:** `documentation` (docs),
  `chore`, `refactor`, `perf`, `test`, `ci`. A `Bug` and a `Feature` carry
  none, because the type already names the commit type.
- **Domain:** `spec:FHIR` (the FHIR terminology wire), `spec:SNOMED` (SNOMED
  CT and RF2 semantics), `spec:ECL` (the ECL parser and evaluator).
- **Area:** `codegen` (the generated FHIR model and its pin), `server`,
  `storage`, `infra`, `website`, `viewer`, `sync`, `addon`. Add more as the
  project grows; keep the set small and meaningful, and add them to
  `scripts/gh/labels.sh` in the same change so a fresh clone converges to the
  taxonomy the tracker uses.
- **Workflow:** `security`, `dependencies` (Dependabot), `accessibility`,
  `blocked-upstream` (waits on an upstream specification or tool release),
  `on-hold` (waits on an input nobody here can provide), `upstream-report`,
  and the pull-request escape hatches `no-changelog` and `no-crate-bump`.
  Blocked-ness is the native blocked-by edge (`issue-relationships.md`), never
  a label.

The organisation also defines `Start date` and `Target date` issue fields;
this repository leaves both empty, because the milestone is the release spine
and its due date is the target (the board's own `Target date` field is the
derived mirror `project-board.md` describes).

## Milestones = releases

A milestone is a delivery promise (`vX.Y.Z`). A release is cut when its
milestone reaches zero open issues (or the owner calls the cut and moves the
stragglers to the next milestone). Every en-route issue goes in the CURRENT
milestone, never the next. New `vX.Y.Z` milestones get a due date (the board's
Roadmap view places items by it, via `scripts/gh/project.sh sync-dates`).

## The fix-first cadence

**An issue filed while working a unit is FIXED before the next unit starts.**
Section findings, verification-pass findings, guard gaps, en-route defects:
filing the issue is the record, never permission to move on. Tractable
findings are fixed in the same branch; genuinely separable work becomes a
linked issue that is closed before the current program advances.

## Branches use conventional types

`<type>/<kebab-case-slug>` with `type` ∈ `feat` | `fix` | `chore` | `docs` |
`refactor` | `perf` | `test` | `ci` | `build` | `release` (the Conventional
Commits type set): e.g. `feat/ecl-descendant-expansion`,
`fix/rf2-refset-parse`. Pick the type by the dominant change; an issue's branch
is normally `feat/<issue-slug>` mirroring the issue's type (a `Feature`) or,
for a `Task`, its work-kind label. Never
force-push `main`.

## Durable vs session-scoped

Issues + git survive `/clear` and `/compact`; the built-in todo tool is
session-scoped, so the tracker is the durable layer. Record progress (tick
checkboxes / comment / open issues for new work) and commit before ending a
session.

## Never add AI/Claude attribution

Commit messages, PR text, issue and comment bodies describe only the change
itself: no `Co-Authored-By: Claude`, no "Generated with", no bot trailer or
footer, ever. This is an absolute rule with no exceptions.
