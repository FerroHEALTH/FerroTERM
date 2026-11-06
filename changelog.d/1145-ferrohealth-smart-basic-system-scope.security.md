- A Basic user is refused a `system/` scope by `ferrohealth-smart` 0.9.1
  (GHSA-jhg2-jwjp-hq74, CWE-269, CWE-863). An `[[auth.basic.users]]` entry
  granted one fails to load in the server and in the sync service, naming
  the user and the scope, and a Basic caller is never handed a `system/`
  scope, the reach of a backend service. FerroTERM refused such a user at
  start with its own check before, which is removed now that the crate
  holds the rule. Upgrade step: none for a configuration that starts today.
