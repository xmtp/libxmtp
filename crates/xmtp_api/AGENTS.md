# xmtp_api

API traits. `default = ["v3"]`. `d14n` feature selects the xmtpd backend.

## Commands

```bash
just check crate xmtp_api
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_api
just test v3 -p xmtp_api --ignore-default-filter publish_identity_update   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_api -E 'test(/identity::/)'"   # one module
```

## Gotchas

- `d14n` feature and `xmtp_api_d14n` get deleted this project. Add nothing d14n-only.
