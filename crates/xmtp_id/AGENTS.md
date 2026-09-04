# xmtp_id

Identity. Inbox IDs, associations, signatures, smart-contract-wallet checks.

## Commands

```bash
just check crate xmtp_id
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_id                 # needs `just backend up` (anvil)
just test v3 -p xmtp_id --ignore-default-filter test_signature_error_verifier_retryable_propagates   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_id -E 'test(/associations::/)'"   # one module
```

## Gotchas

- SCW tests need anvil: `just backend up`.

## Conventions

- `src/lib.rs:InboxOwner`: `get_identifier() -> Result<Identifier, IdentifierValidationError>` and `sign(&str) -> Result<UnverifiedSignature, SignatureError>`; blanket impls for `&T` and alloy's `PrivateKeySigner`. Same file defines `InboxId = String` and `InboxIdRef<'a> = &'a str`.
- Build a `src/associations/member.rs:32 Identifier` with `Identifier::eth(..)`, `::passkey(..)`, `::from_proto(..)`, and call `sanitize()` before trusting input.
- Inbox id derivation: `src/associations/member.rs:171 Identifier::inbox_id(&self, nonce: u64) -> Result<String, AssociationError>`. Its `is_valid_address` check (`:182`) covers Ethereum identifiers only; a passkey passes unchecked. It hashes the displayed identifier plus the nonce with SHA-256, hex-encoded by a private `sha256_string` via `format!("{:x}", ..)`, not `hex::encode`. Always go through this method. Never re-implement the hash.
- State: `associations/state.rs:AssociationState` (immutable updates `add`, `remove`, `set_recovery_identifier`, `diff`), driven by `associations/mod.rs:apply_update` / `get_state`.
- Signature requests: `associations/builder.rs:SignatureRequestBuilder::new(inbox_id)` then `.create_inbox(..)` / `.add_association(..)` / `.revoke_association(..)` then `.build() -> SignatureRequest`, then async `add_signature(sig, scw_verifier)` and `build_identity_update()`.
- Smart-contract wallets: `scw_verifier/mod.rs:84 SmartContractSignatureVerifier` (async `is_valid_signature`, ERC-6492), with `MultiSmartContractSignatureVerifier` and blanket impls for `Arc<T>` / `&T` / `Box<T>`. Take `impl SmartContractSignatureVerifier`, never a concrete verifier.
- Key packages: `src/key_package/verified_key_package_v2.rs`. Do not parse them inline.
- Error coverage is not uniform. `SignatureError` and `IdentifierValidationError` get `ErrorCode` from remote derives in `crates/xmtp_common/src/error_code.rs:48-78`, not from their own crate. `SignerError`, `IdentityError` (`src/lib.rs:20`), and `GroupIdParseError` implement neither `ErrorCode` nor `RetryableError` today. Wrap with `#[from]` rather than stringifying, and add the missing derive if a code must cross the FFI boundary.
