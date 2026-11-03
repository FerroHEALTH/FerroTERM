- `sct-scg` 0.1.129 carries `front`, the parser machinery both SNOMED CT
  languages run on, and `sct-ecl` depends on it. `ParseError`,
  `NESTING_LIMIT`, `LexError`, `Token` and `lex` now live in
  `sct_scg::front` (`Token` and `lex` generic over each crate's
  `lexer::Kind`), and `sct_ecl::ParseError`, `sct_scg::ParseError` and the
  two crates' own `NESTING_LIMIT`, `LexError`, `Token` and `lex` are gone.
  The parsers accept and refuse the same input with the same offsets.
