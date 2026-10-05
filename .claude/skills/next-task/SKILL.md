---
name: next-task
description: Reads the tracker (GitHub Issues), picks the pinned/top open issue (or the issue the user names, skipping blocked ones), and restates it as a concrete in-session work plan naming the files and crates involved. Use when the user asks "what's next" or "what should I work on".
allowed-tools: Read, Grep, Glob, Bash
argument-hint: "[issue number] (optional)"
---

# /next-task

Turns an open tracker issue into an actionable plan, the planning step of the
issue workflow (`.claude/rules/issue-workflow.md`). Does not do the work
itself; that is a separate step the caller takes after seeing the plan.

## Steps

1. **Read the tracker**: `gh issue list --state open` (the SessionStart dump
   annotates each issue with `{k/n}` sub-issue progress, `child-of #parent`,
   and open `BLOCKED-by`/`blocks` edges, and prints `<Type/Priority>` after
   each title). Take the pinned issue (current focus) or the issue the user
   named; otherwise take the open, unblocked issue with the highest `Priority`
   field (`Urgent`, then `High`, `Medium`, `Low`), the oldest first within a
   priority. **Respect relationships**
   (`.claude/rules/issue-relationships.md`): do NOT pick an issue shown
   `BLOCKED-by` an open issue (surface its blocker as the real next task
   instead); for a parent issue, point at its next open child rather than the
   parent itself. An issue shown `<?/?>` has no type or priority yet (a
   scheduled lane filed it): set the type, the priority, the effort and the
   milestone with `scripts/gh/fields.sh` before planning it. Then
   `gh issue view <n> --json title,body,comments --jq '.title, .body,
   (.comments[] | "--- comment ---", .body)'` for the full contract (the opening summary plus
   `## Acceptance criteria`) and the running discussion (`--comments` prints
   nothing for an issue without comments, `issue-workflow.md` §The loop), and
   `scripts/gh/rel.sh tree <n>` for its parent, children, and blockers.
2. **Turn the task into a plan**, stating:
   - **What** the task requires, in one or two sentences.
   - **Which files** are involved, found by searching (Grep/Glob under `crates/`,
     `app/`, `tools/`) rather than guessing paths; if the task names a spec
     surface, resolve it against `docs/architecture.md` (the crate map).
   - **Which mechanism** applies:
     **FHIR wire layer** (the `fhir-types` crate) → **its generator, in the
     FerroBRIDGE repository**: a wrong or missing shape is fixed and released
     there, then taken here as a new version, never re-modelled locally.
     **SNOMED engine** (`rf2`/`concept-graph`/`concept-store`/`designation-index`/
     `sct-ecl`/`fhir-terminology`) or the server → idiomatic modern Rust of
     our own design, the FHIR/SNOMED/ECL specs as the authority (Snowstorm/
     Hermes = behavioural oracles only). Build compiling + tested.
   - **Which spec sections govern it:** for any spec-facing task, name the
     authoritative source (the FHIR operation page for the served version, the
     ECL grammar section, the SNOMED URI spec) the implementation must be read
     against, per `spec-adherence.md` / `/spec-lookup`. Doing the work starts
     by reading those.
   - **What "done" looks like** for this task specifically, the issue's
     `## Acceptance criteria` checklist, plus what proves it: the ECL grammar
     tests, the conformance suite, or a Snowstorm/Hermes oracle comparison.
3. **When work on the picked issue actually starts** (the plan is accepted and
   the session proceeds), move it to `In Progress` on the public roadmap board:
   `scripts/gh/project.sh status <n> in-progress`, the one manual board move
   in the lifecycle (`.claude/rules/project-board.md`). If the session parks
   the issue unfinished, move it back (`scripts/gh/project.sh status <n>
   todo`); a stale In Progress column is a false public claim. (If the board
   does not exist yet, the command fails loud; note that and continue.)
4. **Do not edit the issue or commit:** recording progress happens after the
   work is actually done, not as part of planning it.
