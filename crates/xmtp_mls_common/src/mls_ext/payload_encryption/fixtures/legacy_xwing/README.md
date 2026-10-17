# Legacy XWing Welcome fixture

These are public test keys. Do not use them for messages outside tests.

The fixture uses `hpke-rs` and `hpke-rs-libcrux` 0.6.1 from the XMTP fork,
revision `6dba5c7ff23fa9ad47d56f3207c8f79ff76b67d2`, with
`libcrux-kem` 0.0.9. It was generated before the upgrade to 0.0.10.

The HPKE suite is Base mode, XWing draft 06 at codepoint `0x004D`,
HKDF-SHA256, and ChaCha20-Poly1305. The key derivation input is `[7; 32]`.
The info is the TLS encoding of the MLS encryption context for `MLS_WELCOME`
with an empty context. AAD is empty. The second ciphertext uses the same
HPKE context after the first ciphertext, as Welcome metadata does.

- `public_key.hex` and `private_key.hex` contain the stored recipient keys.
- `wrapped_payload.hex` contains a TLS-encoded `HpkeCiphertext`: the KEM
  output followed by the first ciphertext, each with a TLS variable-length prefix.
- `secondary_ciphertext.hex` contains the second ciphertext without a prefix.

## Generation and upgrade check

`generate.rs` is a standalone harness. Run it in an isolated temporary
Cargo package through `dev/nix-shell`. Use `hpke-rs` from a copy of the
revision above, with the `libcrux`, `hazmat`, and `std` features. Add `hex =
"0.4"` and `serde_json = "1"` to the temporary package.

For this harness only, change the provider's `prng()` constructor in the
copied `libcrux_provider/src/lib.rs` to initialize its `rng` field with
`rand_chacha::ChaCha20Rng::from_seed([42; 32])`. This controls the RNG passed
to XWing encapsulation. The HPKE test RNG seed does not control this path.
Do not make this change in the real provider or enable a test RNG in libxmtp.

Run the old harness and save its JSON output. Run an identical harness
against the upgraded provider with the old JSON path as its first argument.
It must produce identical key bytes, KEM output, payload ciphertext, and
metadata ciphertext. It also decrypts the old ciphertexts. Save its output,
then pass that output to the old harness to check decryption in the reverse
direction. All three runs passed for the upgrade to `libcrux-kem` 0.0.10.

The committed tests use ordinary production randomness. They check that the
current provider opens the old ciphertext, derives the same stored key bytes,
and encrypts new Welcomes under the old public key. To run them:

```sh
dev/nix-shell 'just test workspace -p xmtp_mls_common legacy_xwing'
```
