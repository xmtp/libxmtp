# xmtp_configuration

Constants and tunables. URLs, limits, timeouts.

## Commands

```bash
just check crate xmtp_configuration
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_configuration
just test v3 -p xmtp_configuration --ignore-default-filter centralized_envs_have_api_url   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_configuration -E 'test(/common::/)'"   # one module
```

## Gotchas

- New constants go here. No magic numbers elsewhere.
