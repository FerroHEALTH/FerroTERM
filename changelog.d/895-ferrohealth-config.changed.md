- Breaking: an override variable that names no key refuses the start, naming
  the variable and the closest key it knows (`FERROTERM__…` for the server,
  `FERROTERM_SYNC__…` and `FERROTERM_CONSOLE__…` for the other two), as each
  binary refuses an unknown key in its file. Any other variable under the
  prefix is left alone, so the three binaries can share one environment.
  Upgrade: correct a misspelt override.
- Breaking: a secret file the console reads through a `_file` key loses only
  its trailing line ending, the FerroHEALTH rule, where the console trimmed
  every surrounding space; a file that holds only whitespace other than line
  endings is now a secret, not a refusal. Upgrade: write the secret file with
  the secret alone.
