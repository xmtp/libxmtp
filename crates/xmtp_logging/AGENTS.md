# xmtp_logging

Owns the tracing/OTLP/Sentry pipeline and test subscriber.
Run `dev/nix-shell 'just test crate xmtp_logging'`. Tests use plain attributes
because `xmtp_common` depends on this crate.

- Never add another global subscriber or `tracing-subscriber` in a new crate.
  Tests use scoped `test_logging::LogCapture`.
- New application targets must opt into the explicit filter. Preserve caller
  Sentry component tags and fixed telemetry identity attributes.
- `propagation` owns W3C headers (no-op on WASM); host apps own metric exporters.
  Resource attributes export verbatim: no secrets.
- Backend metrics are independent of sampling; client span metrics are not.
  Configuration and `just backend observe-check`:
  [observability](../../docs/backend-observability.md).
