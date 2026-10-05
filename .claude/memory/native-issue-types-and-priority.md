---
name: native-issue-types-and-priority
description: "Type, priority and effort are GitHub's native issue type and the FerroHEALTH Priority and Effort issue fields, set with scripts/gh/fields.sh; the bug, enhancement, P0 to P3 and blocked labels retire; owner 2026-10-05, the FerroFED model (#704)"
metadata:
  type: project
---

Since 2026-10-05 FerroTERM tracks the kind and urgency of its work the way
FerroFED has since 2026-10-02: the native issue type (`Bug`, `Feature`,
`Task`), the organisation's `Priority` field (`Urgent`, `High`, `Medium`,
`Low`) and its `Effort` field (`High`, `Medium`, `Low`).

**Why:** the owner, 2026-10-05, after the move to the paid FerroHEALTH
organisation: "we need to do an compleet setup change here because look at our
github setup here with native org things", pointing at FerroFED's
`scripts/gh` and its merge queue.

**How to apply:**

- File every issue with `scripts/gh/fields.sh new <type> <priority> <effort>
  <gh issue create args…>`; change one with `fields.sh type|priority|effort
  <n> <value>`; read with `fields.sh show <n>`.
- A Task carries exactly one work-kind label (`documentation`, `chore`,
  `refactor`, `perf`, `test`, `ci`); a Bug or a Feature carries none.
- The migration is `scripts/gh/migrate-fields.sh plan|apply|verify`, run
  before `scripts/gh/labels.sh` deletes the old labels. It moves OPEN issues
  only: the owner chose on 2026-10-05 to leave closed issues as they are,
  accepting that they lose their type and priority with the labels. The auto-mode
  classifier refuses `apply` from an agent session (FerroFED 2026-10-02,
  FerroTERM 2026-10-05), so the owner runs `apply`, `verify` and then
  `labels.sh` by hand.
- Read an issue with `gh issue view <n> --json title,body,comments`; on gh
  2.101.0 `--comments` prints nothing for an issue without comments.
- Related: [[ferrohealth-org-move]], [[auto-merge-every-pr]],
  [[repo-merge-gates]].
