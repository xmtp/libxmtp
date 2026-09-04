# mls_validation_service

gRPC service. Validates key packages, group messages, identity updates.

## Commands

```bash
just check crate mls_validation_service
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate mls_validation_service  # needs `just backend up` (anvil)
just test v3 -p mls_validation_service --ignore-default-filter test_get_association_state   # one test
dev/nix-shell "cargo nextest run --profile ci -p mls_validation_service -E 'test(/handlers::/)'"# needs `just backend up` (anvil)
dev/nix-shell 'cargo run -p mls_validation_service -- --help'
nix build .#validation-service-image     # docker image. `just backend up` does this too.
```

## Gotchas

- Needs `just backend up` (anvil for SCW checks).
- This project turns it into a crate the backend calls in-process. No new standalone features.
- Old tests use `#[tokio::test]`. New tests use `#[xmtp_common::test]`.
