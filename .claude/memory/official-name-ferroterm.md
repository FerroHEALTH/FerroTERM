---
name: official-name-ferroterm
description: The official product name is FerroTERM (site ferroterm.eu); Notio was the codename until 2026-09-02; FerroTERM in prose, ferroterm in technical identifiers
metadata:
  type: project
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

The owner named the product **FerroTERM** on 2026-09-02 and registered `ferroterm.eu` the same day; Notio was the working codename before that. Ferro is the Rust family shared with FerroEHR, TERM is terminology.

**Why:** crates, images, and URLs must not carry a temporary name into publication, and the README needed a stable identity.

**How to apply:** write FerroTERM wherever the product is named in prose (docs, book, landing page, issues, doc comments) and `ferroterm` in every technical identifier (repository, crate names `ferroterm-*`, Rust idents `ferroterm_*`, directories, container image, env vars `FERROTERM_*`, hostnames). Never call it a codename. Related: [[architecture-decisions]].
