- The server and the synchronisation service exit with code 78
  (`EX_CONFIG`, `sysexits(3)`) on a configuration they refuse, as the
  console already did, so an orchestrator tells a bad configuration from a
  failure to serve; `healthcheck` still exits 1. All three binaries run
  through the family run path of ferrohealth-server, which writes a refusal
  as one `<binary>: cannot start: <reason>` line with every cause. Upgrade
  step: a supervisor or script that matched exit code 1 for a refused
  configuration matches 78.
