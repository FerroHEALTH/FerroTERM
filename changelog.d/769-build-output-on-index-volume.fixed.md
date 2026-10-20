- The compose `build` service writes onto the host index directory with no
  `--out`: `docker compose run --rm build --loinc /releases/Loinc_2.83.zip`
  builds into `./index/loinc`. A relative `--out` used to land in the
  container's own filesystem, which `--rm` discarded after the build reported
  success. `ferroterm-build` takes `--index-root` (`FERROTERM_INDEX_ROOT`,
  which the service sets to `/data/index`): without `--out` it writes a child
  of the root named for the source, it refuses an `--out` outside the root,
  and every build prints the absolute path it wrote. The book's Install and
  run page walks through the LOINC build, serving, and a `$lookup` check with
  the host and container paths named, and lists the command for every other
  source (#769).
