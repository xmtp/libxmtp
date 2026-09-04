# xmtp_logging

Production logging. Tracing layers, Sentry backend.

## Commands

```bash
just check crate xmtp_logging
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_logging
just test v3 -p xmtp_logging --ignore-default-filter plain_text_hides_sentry_fields   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_logging -E 'test(/layers::/)'"   # one module
```

## Gotchas

- Test and bench subscribers live in `xmtp_common`, not here.
