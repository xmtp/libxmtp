# xmtp-db-tools

CLI. Migrate, inspect, benchmark XMTP SQLite databases.

## Commands

```bash
just check crate xmtp-db-tools
dev/nix-shell 'dev/agent-run cargo clippy --locked -p xmtp-db-tools --all-targets -- -Dwarnings'
just test crate xmtp-db-tools
just test workspace -p xmtp-db-tools --ignore-default-filter test_bench_works   # one test
just test workspace -p xmtp-db-tools tasks::   # one module
dev/nix-shell 'cargo run -p xmtp-db-tools -- --help'
```

## Gotchas

- In `default-members`. Root `just check`, `just test`, `just lint-rust`, and the
  Rust workspace CI suite include this crate.
- Cargo name is `xmtp-db-tools`, dir is `db_tools`.
