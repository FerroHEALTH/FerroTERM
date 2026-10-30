- The console's `[server] site_root` key and its
  `FERROTERM_CONSOLE__SERVER__SITE_ROOT` variable are gone, and so is the
  release's `ferroterm-console-site` archive: the `ferroterm-console` binary
  carries everything the browser loads. A configuration that still sets
  `site_root` is refused at start as an unknown key, so delete the line
  before you upgrade.
