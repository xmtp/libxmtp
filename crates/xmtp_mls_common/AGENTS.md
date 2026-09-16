# xmtp_mls_common

MLS types shared across crates. Group metadata, permissions, app data.

## Commands

```bash
just check crate xmtp_mls_common
just test crate xmtp_mls_common
just test workspace -p xmtp_mls_common --ignore-default-filter lookup_returns_correct_component_for_each_well_known_id   # one test
just test workspace -p xmtp_mls_common app_data::   # one module
```

## Gotchas

- No Docker or client database dependency. All modules are available on native and wasm.
- General commit-log signing and decoding live in `commit_log`.
- Types the backend also needs go here, not in `xmtp_mls`.
