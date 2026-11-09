---
name: workers-on-opus-only
description: Every subagent runs on Opus, research, planning and review included; Fable is never a worker model, whatever an orchestration table says (owner 2026-10-30)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

Every worker is `model: 'opus'`, including research, hard planning and reviews. Never pass `fable` to `Agent`, whatever a CLAUDE.md orchestration table says about "Fable subagents". Sonnet and Haiku are not used either.

**Why:** the owner, 2026-10-30, after two FerroEHR AQL workers were started on Fable: "always use 5.5 opus is good enough" and "never ever use FABLE never allowed". Fable spends more of the usage budget than the work needs.

**How to apply:** set `model: 'opus'` on every spawn. A task that seems to need more intelligence gets a tighter brief and the previous worker's handover, never a different model. Related: [[no-workflows-for-workers]], [[worker-cap-ten]].
