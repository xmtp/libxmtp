# xmtp_logging

Production logging. Tracing layers, Sentry backend.

## Commands

```bash
just check crate xmtp_logging
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_logging
just test workspace -p xmtp_logging --ignore-default-filter plain_text_hides_sentry_fields   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_logging -E 'test(/layers::/)'"   # one module
```

## Gotchas

- The test subscriber lives here; the `xmtp_common` test macro delegates to it.
- Native tests can use `test_logging::LogCapture` with a scoped dispatch to assert JSON events through the production filter. Do not install another global subscriber.

## Conventions

- `src/lib.rs` owns the whole pipeline: `XmtpLoggingBuilder`, `LoggingHandle`, `filter_directive`, `Level` / `Rotation` / `ProcessType` (`src/config.rs`), OTLP `init` (native only), and the optional Sentry backend (`src/sentry.rs`, feature `sentry`). Never add a second subscriber or pull `tracing-subscriber` into a new crate.
- Component tag: Sentry events carry a `component` tag defaulting to `"libxmtp"`; a caller-supplied one wins (`sentry.rs:148-155`). Set it, do not shadow it.
- The explicit filter includes the backend and shared validation targets. New application crates must opt into this filter before their logs are visible.
