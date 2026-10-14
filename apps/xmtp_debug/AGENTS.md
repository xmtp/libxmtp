# xdbg

CLI to generate identities, groups, and messages against the self-hosted backend.

## Commands

```bash
just check crate xdbg
dev/nix-shell 'dev/agent-run cargo clippy --locked -p xdbg --all-targets -- -D warnings'
just test crate xdbg
dev/nix-shell 'cargo run -p xdbg -- --help'
dev/nix-shell 'cargo run -p xdbg -- generate-api-key'
dev/nix-shell 'cargo run -p xdbg -- --url http://127.0.0.1:5050 generate --entity identity --amount 5'
dev/nix-shell 'cargo run -p xdbg -- --url http://127.0.0.1:5050 query all-key-packages'
dev/nix-shell 'nix run .#xdbg -- --version'
```

## Nix build

- `.#xdbg.unwrapped` compiles with the constant `NIX_GIT_SHA=nix`, so its
  store path does not change with the commit, and Cachix serves it across commits.
- `.#xdbg` wraps it and always sets `XDBG_GIT_SHA` to the build's commit (the
  flake's short revision), so an inherited value cannot override it.
- The version also selects the storage bucket (the redb identity key and the
  SQLite directory). `get_version()` reads `XDBG_GIT_SHA` only in the Nix
  compile, whose compile-time SHA is the placeholder `nix`. Cargo builds ignore
  the variable and keep their vergen SHA.
- Do not read `self.rev` or the commit date in the compile.
  `just backend observe-check` uses `.#xdbg` and checks that `--version` names
  the commit.

## Backend options

- `--url` (or `-u`) is required for app commands. There is no default URL.
- `generate-api-key` needs no URL. It prints only a 256-bit random key as hex,
  then exits before logging, metrics, or local storage starts.
- The URL hash selects the storage directory. Each URL has separate local state.
- Backend selection, gateway, payer, perf, migration, and decentralization options are removed.
- The migration test scenarios and the generation read-own-writes option are removed.
- Queries use `ApiClientWrapper`. Streams use the `xmtp_mls` stream methods.
- `MessageBackendBuilder` builds the API client. Do not add a local builder.
- The Docker monitor requires `XMTP_BACKEND_URL`.
- `--otel-endpoint <URL>` exports traces through the shared OTLP pipeline. The logger holds the provider guard until exit.
- `just backend observe-check` uses this flag to check client and backend spans
  in one Tempo trace. It creates two identities, one group, and ten messages.
  `XDBG_DB_ROOT` isolates its temporary state. `--fail-fast` makes operation errors fail the check.
  See [backend observability](../../docs/backend-observability.md).
- `--metrics` keeps CSV output. `PUSHGATEWAY_URL` keeps optional Prometheus output.

## Gotchas

- Run `just backend up` before network commands.
- Not in `default-members`. Root `just check`, `just test`, and `just lint-rust` skip this crate.
