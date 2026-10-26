- The server, the synchronisation service and the console read the
  FerroHEALTH `[telemetry]` section: `format` and `filter` as before (with
  `RUST_LOG` setting the filter in all three), `logged_query_parameters` for
  further query parameters the request line may carry, `environment`, and a
  `[telemetry.otlp]` table that pushes the spans and the metrics to an
  OpenTelemetry collector over OTLP/gRPC (`endpoint`, `traces`, `metrics`,
  `sample_ratio`, `trust_roots_file`, `client_identity_file`). A panic is
  logged as one line before the process ends.
