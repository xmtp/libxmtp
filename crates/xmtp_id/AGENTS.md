# xmtp_id

Identity, associations, signatures, and smart-contract wallets.
Run `dev/nix-shell 'just test crate xmtp_id'` after `just backend up` through Nix
(SCW tests need Anvil).

- Before constructing or validating identifiers, read [identity guidance](../../.agents/skills/writing-rust/types.md#identity).
  Derive inbox IDs with `Identifier::inbox_id`, never a duplicated hash.
  Parse key packages via `key_package`, not inline.
- Take `impl SmartContractSignatureVerifier`, not a concrete verifier.
- Keep errors typed with `#[from]`; ensure codes needed across FFI are derived
  (some use remote derives in `xmtp_common`). See [error architecture](../../docs/error-architecture.md).
- `test-utils` is portable; native Anvil fixtures require `test-utils-anvil`.
  `ANVIL_URL` overrides the worktree test address.
