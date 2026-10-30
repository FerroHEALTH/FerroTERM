- The console moves to FerroUI 0.6 and draws every page on its own server:
  there is no WebAssembly and no Leptos in it any more, every page works with
  scripts turned off, and a link or a form loads the next page whole. Its
  Content-Security-Policy is `script-src 'self'; style-src 'self'` with no
  nonce and no `'wasm-unsafe-eval'`, and FerroUI's stylesheet, script modules
  and brand files are served by the console binary under
  `/ferrohealth-ui/0.6.0/`, cached as immutable. A reverse proxy that
  forwarded the old `/pkg/`, `/ferrohealth-ui.css` and `/theme.css` paths
  can drop them.
- The console's own keyboard shortcuts (the `g` pairs that open a screen,
  `Ctrl Enter` to run an operation, `Ctrl S` to save an editor) are gone
  until FerroUI offers them as a module (#945); `/`, `?`, Escape and the
  concept tree's keys work as before, and every action keeps its button. The
  typed preferences (default FHIR version, display language, page size) are
  plain forms that save with their own button (#946).
