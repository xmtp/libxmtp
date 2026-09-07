# mls_validation_service

gRPC service. Validates key packages, group messages, identity updates.

## Commands

```bash
just check crate mls_validation_service
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate mls_validation_service  # needs `just backend up` (anvil)
just test v3 -p mls_validation_service --ignore-default-filter test_validate_scw   # one test
dev/nix-shell "cargo nextest run --profile ci -p mls_validation_service -E 'test(/handlers::/)'"# needs `just backend up` (anvil)
dev/nix-shell 'cargo run -p mls_validation_service -- --help'
nix build .#validation-service-image     # docker image. `just backend up` does this too.
```

## Gotchas

- Needs `just backend up` (anvil for SCW checks).
- Payload admission is in `xmtp_mls_validation`; the SCW cache is in `xmtp_id`.
- The legacy release Dockerfile uses Debian Bookworm for both stages. Keep builder
  and runtime bases aligned and supported; never bypass apt metadata expiry checks.
- This transport shell remains until Phase 3 SDK integration. Do not add standalone features.
- Tests use `#[xmtp_common::test(unwrap_try = true)]`.
