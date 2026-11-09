- The console no longer names `[ferroterm] timeout_ms` when it refuses the
  key: the 0.4.4 refusal is gone, and the key is now an unknown key that the
  strict load refuses like any other. Upgrade step: write `[ferroterm]
  timeout` as a duration such as `"30s"`.
