# xdbg

CLI (`xdbg`). Generate identities, groups, messages against a network. Cross-version harness.

## Commands

```bash
just check crate xdbg
dev/nix-shell 'cargo clippy --locked -p xdbg --all-targets -- -Dwarnings'
just test crate xdbg
just test v3 -p xdbg --ignore-default-filter empty_input_returns_empty   # one test
dev/nix-shell "cargo nextest run --profile ci -p xdbg -E 'test(/app::/)'"   # one module
dev/nix-shell 'cargo xdbg --help'
```

## Gotchas

- Not in `default-members`. Root `just check`, `just test`, `just lint-rust` skip this crate.
