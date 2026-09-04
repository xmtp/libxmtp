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
