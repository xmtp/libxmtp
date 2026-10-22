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
just validation check                  # standalone native/wasm consumer and dependency checks
just validation test                   # native/wasm payload tests and canonical encoding vectors
just validation                        # both
just test workspace -p xmtp_mls_validation --features test-utils
just test workspace -p xmtp_mls_validation --test zeroize   # commit-log signer heap-canary proof
dev/nix-shell 'dev/agent-run cargo test --locked -p xmtp_mls_validation --target wasm32-unknown-unknown --features test-utils'
```

Use `test-utils` for fixtures. Parse routing before cryptographic validation so
duplicate retries do not depend on the current key-package lifetime. Parsing
checks the length of every field a publish signature covers; `verify_publish`
checks the signature itself, after the caller's duplicate lookup.
`group_keys`, `keyed_group_message_envelope`, and `sealed_commit_log_envelope`
build signed fixtures.

After dependency changes, run `just validation check`. Workspace builds can hide
missing features through feature unification. Keep `xmtp_mls/test-utils`,
database dependencies, and native network/Anvil fixtures out of this crate.

Run one test with `just test workspace -p xmtp_mls_validation group_message_matrix`.
