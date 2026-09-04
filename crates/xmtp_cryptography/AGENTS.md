# xmtp_cryptography

Signatures, hashing, key types.

## Commands

```bash
just check crate xmtp_cryptography
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_cryptography
just test v3 -p xmtp_cryptography --ignore-default-filter test_public_key_generation_and_address   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_cryptography -E 'test(/ethereum::/)'"   # one module
```

## Gotchas

- Pure. No docker.

## Conventions

This crate owns every primitive. `crates/xmtp_common/src/lib.rs` re-exports its `hash` and `rand` modules, so prefer the `xmtp_common::` path.

- Hashing (`src/hash.rs`): `sha256_bytes(&[u8]) -> Vec<u8>`, `sha256_array(&[u8]) -> [u8; 32]` (allocation-free). There is no plain `sha256` here or in `xmtp_common`. The alias at `crates/xmtp_mls/src/utils/mod.rs:15` is legacy. Do not use it in new code.
- Native entry points must call `src/lib.rs:29 install_crypto_provider()` first. It installs the process-default rustls provider and is idempotent. Its `#[ctor]` fallback does not fire when the static library is linked into an Apple binary, so a `reqwest` client built before this call panics with "No provider set" (`bindings/node/src/client/create_client.rs:231`, `bindings/mobile/src/mls.rs:152`). Native only.
- Randomness (`src/rand.rs`, ChaCha20-backed): `rng()`, `seeded_rng(seed)`, `rand_string::<N>()`, `rand_array::<N>()`, `rand_vec::<N>()`, `rand_secret::<N>()`.
- Signatures (`src/signature.rs`): `SignatureError`, `RecoverableSignature::recover_address`, `is_valid_ethereum_address`, `sanitize_evm_addresses`, `h160addr_to_string`.
- Installation credentials (`src/basic_credential.rs`, re-exported at the crate root): `XmtpInstallationCredential`, traits `CredentialSign` / `CredentialVerify` / `SigningContextProvider`. Ciphersuites and key lengths: `src/configuration.rs` (`CIPHERSUITE`, `ED25519_KEY_LENGTH`). Test wallet: `src/utils.rs:generate_local_wallet()`.
