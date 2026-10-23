- The console answers a request for a site-bundle file under `/pkg/` that is not there with a plain `404 Not Found` and no body, where it drew a page of the shell, which a browser loading its WebAssembly read as the module.
  The bundle is served from the site root on its own route, so a miss never reaches the page fallback.
