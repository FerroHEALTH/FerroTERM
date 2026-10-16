- The `build` service in the release `compose.yaml` builds any code system
  `ferroterm-build` knows, so a deployment without a SNOMED CT licence builds
  LOINC or another system through Compose. It mounts a release directory
  (`./releases`, or `FERROTERM_RELEASES_DIR`) at `/releases` and takes the
  builder's own arguments:
  `docker compose run --rm build --loinc /releases/Loinc_2.82.zip --out /data/index/loinc`.
  `FERROTERM_RF2` is gone; a SNOMED CT build passes
  `--rf2 /releases/<zip> --out /data/index/snomed` (#713).
