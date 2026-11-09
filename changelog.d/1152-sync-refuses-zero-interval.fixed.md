- `ferroterm-sync` refuses to start with a `schedule.every` interval of zero
  or less and names the key and the value. It used to accept one and start
  each run the moment the last one ended, writing a run record each time.
