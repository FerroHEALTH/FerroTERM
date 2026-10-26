- Breaking: every variable that starts with `FERROTERM_` and is not one of the
  binary's own refuses its start, naming the variable and the closest name it
  knows: the server refuses a `FERROTERM_CONSOLE_` or `FERROTERM_SYNC_`
  variable in its environment, as each binary refuses an unknown key in its
  file. Upgrade: give each process only its own variables.
- Breaking: a secret file the console reads through a `_file` key loses only
  its trailing line ending, the FerroHEALTH rule, where the console trimmed
  every surrounding space; a file that holds only whitespace other than line
  endings is now a secret, not a refusal. Upgrade: write the secret file with
  the secret alone.
