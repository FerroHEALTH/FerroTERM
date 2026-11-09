---
name: subagent-reports-to-file
description: A subagent writes its long report to a file in its own scratch subfolder and replies with the path, because a truncated result is lost and a finished agent cannot be resumed (2026-09-12)
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

On 2026-09-12 two long research reports came back truncated, and messages to the finished agents failed with "No transcript found", so half of each report had to be recovered by script.

**Why:** the orchestrator only sees the final message, which is cut at a size limit, and a finished agent is not resumable. Session transcripts are not a durable store.

**How to apply:** every research or register brief tells the agent to write its full report to a named file under its own scratch subfolder ([[worker-scratch-subfolders]]) and to reply with the path; the orchestrator reads the file and copies what must last into the repository or onto the issue.
