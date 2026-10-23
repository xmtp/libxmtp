# xmtp_cryptography

Shared crypto primitives. Pure tests, no services; use `#[test]` to avoid the
`xmtp_common` dependency cycle. Run `dev/nix-shell 'just test crate xmtp_cryptography'`.

- Import secret/zeroization types through `secret`; every owned secret copy needs
  a wiping owner and redacted Debug. Read [secret rules](../../.agents/skills/writing-rust/types.md#secrets)
  and [Secret's serde contract](src/secret.rs) before changing secret handling.
- Heap-canary proofs use `secret::heap_canary` (`test-utils`, Unix). Install the
  allocator in a separate test binary with required features; scans cannot overlap.
- Native entry points must call idempotent `install_crypto_provider()` before
  clients: the ctor fallback does not run in Apple static libraries.
- Reuse `sha256_bytes` / `sha256_array`, random, signature, and credential helpers;
  do not add duplicate primitives or use the legacy `sha256` alias.
