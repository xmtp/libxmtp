# xmtp_mls_validation

Shared payload admission and stateless fixtures. No client database or transport.

General commit-log signing and decoding live in `xmtp_mls_common::commit_log`.
Admission parsing and validation stay in this crate.

```bash
just check crate xmtp_mls_validation
just test crate xmtp_mls_validation
just check-validation                  # standalone native/wasm consumer and dependency checks
just test-validation                   # native/wasm payload tests and canonical encoding vectors
dev/nix-shell 'cargo build --locked -p xmtp_mls_validation'
dev/nix-shell 'cargo test --locked -p xmtp_mls_validation --features test-utils'
dev/nix-shell 'cargo test --locked -p xmtp_mls_validation --target wasm32-unknown-unknown --features test-utils'
just lint-rust
```

Use `test-utils` for fixtures. Parse routing before cryptographic validation so
duplicate retries do not depend on the current key-package lifetime.

Run one test with `dev/nix-shell 'cargo test --locked -p xmtp_mls_validation group_message_matrix'`.
