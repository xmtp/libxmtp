# xmtp_api_grpc

gRPC transport for the `xmtp_api` traits.

## Commands

```bash
just check crate xmtp_api_grpc
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_api_grpc
dev/nix-shell 'cargo test --locked -p xmtp_api_grpc grpc_client::client::' # local header and transport tests
dev/nix-shell 'cargo test -p xmtp_api_grpc does_not_starve_s2'   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_api_grpc -E 'test(/streams::/)'"   # one module
```

## Gotchas

- Needs `just backend up` (`backend`, `toxiproxy`).
- `test-utils` pulls `toxiproxy_rust`. Fault tests talk to the `toxiproxy` container.
