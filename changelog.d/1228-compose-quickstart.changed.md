- The quick start downloads the release's self-contained `compose.yaml`,
  now attached with its `.sha256sum`, and starts it with an empty index
  directory in place of a `docker run` of the image. CI runs the README's
  quick start over the repository's `compose.yaml`, the file each release
  attaches, and refuses a host path it would read from the repository
  ([#1228](https://github.com/FerroHEALTH/FerroTERM/issues/1228)).
