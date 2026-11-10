- The synchronisation service calls the Nationale Terminologie Server
  through the family outbound client of ferrohealth-smart 0.17, built
  from `[nts]`, which is now the family endpoint table: `url`, `timeout`
  (`"1h"`, since a download of a whole edition is one call),
  `connect_timeout` (`"30s"`), `retry`, `tls` and the client credentials
  grant under `[nts.credentials.oauth]`, whose `issuer`, `client_auth =
  "client_secret_post"` and `refresh_margin = "1m"` default to the
  service's realm. A listing, a page of the FHIR API or a download that did
  not connect, timed out or met a `5xx` is tried again as `retry` allows.
  In 0.4.5 alone each earlier key refuses the start naming its
  replacement. Upgrade step: write `[nts.credentials.oauth] client_id`,
  `client_secret` and `client_secret_file` for `[nts] client_id`,
  `client_secret` and `client_secret_file`
  (`FERROTERM_SYNC__NTS__CREDENTIALS__OAUTH__CLIENT_ID` and so on), and
  `[nts] url` and `[nts.credentials.oauth] issuer` for a `[[source]]`
  block's `base_url` and `issuer`.
