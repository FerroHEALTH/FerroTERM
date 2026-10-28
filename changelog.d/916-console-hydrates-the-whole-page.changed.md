- The console runs on FerroUI 0.5.6 and the browser hydrates the whole
  page: after the first page, a link, a `GET` form, a preference and the
  navigation rail draw the next page in place, with no document reload. The
  pages load in five groups as chunks of their own. The console's server
  reads each page once, recording the requests it sends the terminology
  server, and the browser draws the page from that recording, so a page
  after a navigation costs one round trip to the console. FerroUI's
  keyboard shortcut legend replaces the console's own, and a Configuration
  value set by a `--set` argument now says "From the command line" in place
  of "From the environment".
