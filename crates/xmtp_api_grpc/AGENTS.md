# xmtp_api_grpc

gRPC transport for the `xmtp_api` traits.

## Commands

```bash
just check crate xmtp_api_grpc
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_api_grpc
just test v3 -p xmtp_api_grpc --ignore-default-filter does_not_starve_s2   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_api_grpc -E 'test(/streams::/)'"   # one module
```

## Gotchas

- Needs `just backend up` (`node`, `toxiproxy`).
- `test-utils` pulls `toxiproxy_rust`. Fault tests talk to the `toxiproxy` container.
