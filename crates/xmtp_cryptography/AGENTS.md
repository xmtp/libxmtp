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
