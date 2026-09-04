# keepalive-probe

CLI. Holds gRPC connections to an endpoint. Measures keepalive survival.

## Commands

```bash
just check crate keepalive-probe
dev/nix-shell 'cargo clippy --locked -p keepalive-probe --all-targets -- -Dwarnings'
just test crate keepalive-probe
just test v3 -p keepalive-probe --ignore-default-filter percentile_basics   # one test
dev/nix-shell 'cargo run -p keepalive-probe -- --help'
```

## Gotchas

- Not in `default-members`. Root `just check`, `just test`, `just lint-rust` skip this crate.
