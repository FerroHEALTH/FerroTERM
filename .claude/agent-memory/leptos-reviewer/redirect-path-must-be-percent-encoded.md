---
name: redirect-path-must-be-percent-encoded
description: the leptos_axum redirect panic is gone with the server, but the underlying rule grows teeth here, because system URIs, ECL, and concept ids all land in FHIR request URLs
csr: changed
metadata:
  type: reference
---

<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

**The original finding.** `leptos_axum::redirect` builds the `Location` header
with `HeaderValue::from_str(path).expect("Failed to create HeaderValue")`.
Header values reject control characters and non-ASCII bytes, so an unencoded
user value interpolated into a redirect path was a remotely triggerable panic
from a plain query string such as `?find=%0Aevil`. Percent-encoding closed it,
because the encoder emits only `[0-9A-Za-z\-._~]` plus `%XX`, which also keeps
the value inside its intended path segment.

**Moot half.** There is no `leptos_axum` and no server-side redirect in this
viewer, so that specific panic cannot happen.

**The half that grows teeth.** Every value this viewer puts into a URL is
hostile-shaped by nature, and it puts a lot of them there:

- a code system URI is a URL (`http://snomed.info/sct`), so it carries `:` and
  `/` that are structural in both a path segment and a query value;
- an ECL expression carries `<`, `>`, `|`, `{`, `}`, `:`, `,`, and spaces;
- an implicit value set canonical carries `?` and `=`
  (`http://snomed.info/sct?fhir_vs=isa/404684003`), so an unencoded one
  truncates the query it is embedded in;
- a search term is free text in any language.

So: **percent-encode every value that lands in a request URL or a route path**,
in `crate::fhir` and in every link builder, and never `format!` a raw value into
either. Getting this wrong does not panic here; it silently sends a different
request than the reader asked for, which is worse. Note that trimming strips
only leading and trailing whitespace, so an interior newline still reaches the
URL.

leptos_router percent-decodes params on read (`ParamsMap::insert` calls
`Url::unescape`, `src/params.rs:29`), so the round-trip through a link is
lossless.

Related: [[leptos-router-form-interception]], [[redirect-needs-ssrmode-async]]
