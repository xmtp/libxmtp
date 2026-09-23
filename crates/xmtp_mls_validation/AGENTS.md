# xmtp_mls_validation

Shared payload admission, commit validation rules, and stateless fixtures.
No client database or transport.

`commit` holds the rules `xmtp_mls` applies to a staged commit, with
`group_permissions` and `group_membership`. `xmtp_mls` keeps the checks that
need the database or identity proofs; `CommitRuleError` is the rule layer of
its `CommitValidationError`.

General commit-log signing and decoding live in `xmtp_mls_common::commit_log`.
Admission parsing and validation stay in this crate.

```bash
just check crate xmtp_mls_validation
just test crate xmtp_mls_validation
just check-validation                  # standalone native/wasm consumer and dependency checks
just test-validation                   # native/wasm payload tests and canonical encoding vectors
just test workspace -p xmtp_mls_validation --features test-utils
dev/nix-shell 'dev/agent-run cargo test --locked -p xmtp_mls_validation --target wasm32-unknown-unknown --features test-utils'
```

Use `test-utils` for fixtures. Parse routing before cryptographic validation so
duplicate retries do not depend on the current key-package lifetime.

After dependency changes, run `just check-validation`. Workspace builds can hide
missing features through feature unification. Keep `xmtp_mls/test-utils`,
database dependencies, and native network/Anvil fixtures out of this crate.

Run one test with `just test workspace -p xmtp_mls_validation group_message_matrix`.
