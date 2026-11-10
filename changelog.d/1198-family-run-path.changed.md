- The server and the synchronisation service exit with code 78
  (`EX_CONFIG`, `sysexits(3)`) on a configuration they refuse, as the
  console already did, so an orchestrator tells a bad configuration from a
  failure to serve. `healthcheck` on a configuration that does not load
  writes `<binary>: not up: <reason>` and exits 1. All three binaries run
  through the family run path of ferrohealth-server 0.8.1: one
  `<binary>: cannot start: <reason>` line with every cause on a refusal,
  `config check` printing the running tree with every secret redacted, and
  the telemetry started and flushed by the family. Upgrade step: a
  supervisor or script that matched exit code 1 for a refused configuration
  matches 78.
