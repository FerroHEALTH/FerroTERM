- The server, the synchronisation service and the console read their
  configuration through the FerroHEALTH loader, `ferrohealth-config`: the
  defaults, a TOML file named by `--config` or by `FERROTERM_CONFIG`,
  `FERROTERM_SYNC_CONFIG` or `FERROTERM_CONSOLE_CONFIG`, one variable per key
  (`FERROTERM__SECTION__KEY`, `FERROTERM_SYNC__SECTION__KEY`,
  `FERROTERM_CONSOLE__SECTION__KEY`), and `--set key=value` last. `ferroterm
  config check` and `ferroterm-sync config check` print every key with where it
  came from and every secret as `***`.
