# xdbg

CLI to generate identities, groups, and messages against the self-hosted backend.

## Commands

```bash
just check crate xdbg
dev/nix-shell 'cargo clippy --locked -p xdbg --all-targets -- -D warnings'
just test crate xdbg
dev/nix-shell 'cargo run -p xdbg -- --help'
dev/nix-shell 'cargo run -p xdbg -- --url http://127.0.0.1:5050 generate --entity identity --amount 5'
dev/nix-shell 'cargo run -p xdbg -- --url http://127.0.0.1:5050 query all-key-packages'
```

## Backend options

- `--url` (or `-u`) is required. There is no default URL.
- The URL hash selects the storage directory. Each URL has separate local state.
- Backend selection, gateway, payer, perf, migration, and decentralization options are removed.
- The migration test scenarios and the generation read-own-writes option are removed.
- Queries use `ApiClientWrapper`. Streams use the `xmtp_mls` stream methods.
- `MessageBackendBuilder` builds the API client. Do not add a local builder.
- The Docker monitor requires `XMTP_BACKEND_URL`.
- `--metrics` keeps CSV output. `PUSHGATEWAY_URL` keeps optional Prometheus output.

## Gotchas

- Run `just backend up` before network commands.
- Not in `default-members`. Root `just check`, `just test`, and `just lint-rust` skip this crate.
