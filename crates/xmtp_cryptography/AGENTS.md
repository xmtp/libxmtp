# xmtp_cryptography

Signatures, hashing, key types.

## Commands

```bash
just check crate xmtp_cryptography
just test crate xmtp_cryptography
just test workspace -p xmtp_cryptography --ignore-default-filter test_public_key_generation_and_address   # one test
just test workspace -p xmtp_cryptography ethereum::   # one module
just test workspace -p xmtp_cryptography --features test-utils --test heap_canary   # heap-canary self-test
```

## Gotchas

- Pure. No docker.
- Tests use `#[test]`, not `#[xmtp_common::test]`: `xmtp_common` depends on this crate.

## Conventions

This crate owns every primitive. `xmtp_common` re-exports the hash and random functions at its root.

- Secrets (`src/secret.rs`): the one import path for zeroization. `Secret` owns secret bytes (wiped on drop, `Debug` shows only the length, serde equals `Vec<u8>`, constant-time `==`). `Zeroizing<T>` wraps transient buffers where they are made. `Zeroize` and `ZeroizeOnDrop` are re-exported. Every heap copy of a private key, HMAC key, webhook signing key or push recipient secret that libxmtp owns must sit in one of these, in an allowed dependency key type (`ed25519_dalek::SigningKey`, `k256` and alloy signing keys, OpenMLS `SignatureKeyPair` and `HpkePrivateKey`), or in a struct with a wiping `Drop`. A struct that holds bare secret bytes (`Vec<u8>`, `[u8; N]`, `String`, `Zeroizing<T>`) or an OpenMLS HPKE key never derives `Debug`; one whose secret fields all redact in `Debug` (`Secret`, `StoredSecret`, the signing-key types above) may. The rule and its exceptions are in the `writing-rust` skill (`types.md`, "Secrets").
- Heap canary (`src/secret/heap_canary.rs`, feature `test-utils`, native only): the one test allocator for zeroization proofs. A test binary calls `install_heap_canary!()`, is a `[[test]]` with `required-features = ["test-utils"]`, and asserts zero hits from `scan` / `scan_async`.
- Hashing (`src/hash.rs`): `sha256_bytes(&[u8]) -> Vec<u8>`, `sha256_array(&[u8]) -> [u8; 32]` (allocation-free). There is no plain `sha256` here or in `xmtp_common`. The alias at `crates/xmtp_mls/src/utils/mod.rs:15` is legacy. Do not use it in new code.
- Native entry points must call `install_crypto_provider()` (`src/lib.rs`) first. It installs the process-default rustls provider and is idempotent. Its `#[ctor]` fallback does not fire when the static library is linked into an Apple binary, so a `reqwest` client built before this call panics with "No provider set" (`crates/xmtp_sdk/src/client.rs`). Native only.
- Randomness (`src/rand.rs`, ChaCha20-backed): `rng()`, `seeded_rng(seed)`, `rand_string::<N>()`, `rand_array::<N>()`, `rand_vec::<N>()`, `rand_secret::<N>()`.
- Signatures (`src/signature.rs`): `SignatureError`, `RecoverableSignature::recover_address`, `is_valid_ethereum_address`, `sanitize_evm_addresses`, `h160addr_to_string`.
- Installation credentials (`src/basic_credential.rs`, re-exported at the crate root): `XmtpInstallationCredential`, traits `CredentialSign` / `CredentialVerify` / `SigningContextProvider`. Ciphersuites and key lengths: `src/configuration.rs` (`CIPHERSUITE`, `ED25519_KEY_LENGTH`). Test wallet: `src/utils.rs:generate_local_wallet()`.
