# xmtp_common

Shared helpers. Owns `#[xmtp_common::test]`, retry, time, rand.

## Commands

```bash
just check crate xmtp_common
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_common
just test v3 -p xmtp_common --ignore-default-filter bundled_roots_config_is_accepted_by_reqwest   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_common -E 'test(/http::/)'"   # one module
```

## Gotchas

- Every crate depends on it. A change rebuilds the workspace.
- Shared helpers go here. Never copy a helper into another crate.
