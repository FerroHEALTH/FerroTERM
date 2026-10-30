<!-- SPDX-FileCopyrightText: Cadasto B.V. -->
<!-- SPDX-License-Identifier: BUSL-1.1 -->

# FerroTERM Console

The FerroTERM Console is the browser interface to a FerroTERM terminology
server. It renders every page on its own server and reaches FerroTERM over the
same public FHIR API any client uses, so anything it does, your own client can
do. It is one of the FerroHEALTH consoles and draws with their shared
components.

This archive holds the console server, `ferroterm-console`. The one binary
carries everything the browser loads; there is no separate site bundle to
unpack.

## Run

The quickest way is the container image, through the quickstart's `console`
profile, which runs it beside the server and opens on `http://localhost:3000`:

```console
docker compose --profile console up
```

To run this binary yourself, point it at your FerroTERM server and give it its
configuration. The [FerroTERM book](https://ferroterm.eu/docs/) describes the
configuration, the sign-in, and the reverse proxy in front of it.

## Licence

The Business Source License 1.1 (`LICENSE`, `NOTICE` in this archive): free for
non-production use and for non-commercial production use, a commercial licence
from Cadasto B.V. for other production use, and the Apache License 2.0 four
years after each version.
