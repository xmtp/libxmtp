# xmtp-db-tools

CLI. Migrate, inspect, benchmark XMTP SQLite databases.

## Commands

```bash
just check crate xmtp-db-tools
dev/nix-shell 'cargo clippy --locked -p xmtp-db-tools --all-targets -- -Dwarnings'
just test crate xmtp-db-tools
just test v3 -p xmtp-db-tools --ignore-default-filter test_bench_works   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp-db-tools -E 'test(/tasks::/)'"   # one module
dev/nix-shell 'cargo run -p xmtp-db-tools -- --help'
```

## Gotchas

- Not in `default-members`. Root `just check`, `just test`, `just lint-rust` skip this crate.
- Cargo name is `xmtp-db-tools`, dir is `db_tools`.
