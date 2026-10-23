# xmtp_mls_validation

Shared stateless admission/commit rules; no client DB or transport. Checks needing
DB/identity proofs stay in `xmtp_mls`; commit-log crypto stays in `xmtp_mls_common`.

```sh
dev/nix-shell 'just test crate xmtp_mls_validation'
dev/nix-shell 'just validation' # standalone native/WASM checks + payload vectors
```

- Parse routing before crypto validation so duplicate retries do not depend on
  current key-package lifetime. Parsing checks signed-field lengths; after
  duplicate lookup, run `verify_publish` for each `is_signed()` envelope.
  `validate_envelope` does not verify publish signatures.
- Backend group-message/commit-log fixtures must be signed; use
  [signed fixture helpers](src/test_utils.rs). `group_message_envelope` is
  unsigned; retired field-5 entries decode without a payload. Both are for malformed cases.
- Fixtures use `test-utils`. Keep `xmtp_mls/test-utils`, DB, native network, and
  Anvil dependencies out. After dependency edits, run `just validation check`
  through Nix: workspace feature unification can hide missing features.
