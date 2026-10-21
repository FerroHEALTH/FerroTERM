---
name: sonar-mcp-triage
description: the owner wants every SonarQube Cloud finding triaged and fixed through the SonarQube MCP server (issue #209, 2026-09-04); the analyzer stays advisory
metadata:
  type: feedback
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner asked on 2026-09-04 for a proper issue to resolve every open SonarQube Cloud finding (182 on `main` then: 13 security, 82 reliability, 87 maintainability, quality gate failing) and wants the work done through the SonarQube MCP server so findings can be read and marked directly from the session. Filed as #209.

**Why:** the dashboard is public and a failing gate reads as a project in bad shape; the owner values a clean, lean surface.

**How to apply:** when the SonarQube MCP server is configured, work #209 finding by finding: read each against the specs and the hard rules first (`.claude/rules/ai-code-review.md`: advisory, never authority), fix the right ones in normal changes, record the wrong ones with a citation, never an unexplained suppression. Related: [[repo-merge-gates]].
