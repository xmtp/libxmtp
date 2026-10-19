# xmtp_mls_common

MLS types shared across crates. Group metadata, permissions, app data.

## Commands

```bash
just check crate xmtp_mls_common
just test crate xmtp_mls_common
just test workspace -p xmtp_mls_common --ignore-default-filter lookup_returns_correct_component_for_each_well_known_id   # one test
just test workspace -p xmtp_mls_common app_data::   # one module
just test workspace -p xmtp_mls_common --features test-utils --test zeroize   # commit-log signer heap-canary proofs
```

## Gotchas

- No Docker or client database dependency. All modules are available on native and wasm.
- General commit-log signing and decoding live in `commit_log`.
- The commit-log signer is a private key. `GroupMutableMetadata` redacts and wipes its `_commit_log_signer` attribute; change attributes with `insert_attribute` and `remove_attribute`, which wipe the old value. Hold signer bytes as `Secret` or `Zeroizing` until they reach OpenMLS.
- Types the backend also needs go here, not in `xmtp_mls`.
