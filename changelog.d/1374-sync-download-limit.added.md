- `ferroterm-sync` takes at most `download_limit` bytes per download, `4GiB`
  by default: a response whose `Content-Length` passes it is refused before
  anything is written, and a body that passes it while it streams is cut off,
  with its partial file removed and the run record naming the item.
