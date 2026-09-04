# xmtp_mls_common

MLS types shared across crates. Group metadata, permissions, app data.

## Commands

```bash
just check crate xmtp_mls_common
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_mls_common
just test v3 -p xmtp_mls_common --ignore-default-filter lookup_returns_correct_component_for_each_well_known_id   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_mls_common -E 'test(/app_data::/)'"   # one module
```

## Gotchas

- Pure. No docker.
- Types the backend also needs go here, not in `xmtp_mls`.
