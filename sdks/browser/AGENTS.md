# XMTP Browser SDK

```sh
dev/nix-shell 'just js test-browser-sdk-ci'
dev/nix-shell 'just sdk test-browser' # platform proofs, omitted by pnpm test
```

- Each test owns a client; await `client.end()` before restore/storage replacement.
- The generated package owns the worker and OPFS pool. Do not add another worker
  or storage dispatcher. Synchronous codecs use `/pure`.
- Stage with `dev/js/sdk-package browser`; publish `target/sdk-packages/browser`.
- Platform proofs require generated fixtures; see
  [SDK test setup](../../crates/xmtp_sdk/DEVELOPMENT.md). Use
  `just sdk test-browser migration` through Nix for migration-only proofs.
- Migration archive reads have an inclusive 100 MB limit; check size before
  `arrayBuffer` allocation. Keep the real OPFS boundary proof.
