# xmtp_proto

Generated protobuf types and gRPC stubs.

## Commands

```bash
just check crate xmtp_proto
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_proto
just test v3 -p xmtp_proto --ignore-default-filter test_is_commit   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_proto -E 'test(/types::/)'"   # one module
dev/nix-shell 'dev/gen_protos.sh'         # regen from xmtp/proto main
```

## Gotchas

- Generated. Never hand-edit `src/gen/`.
- Regenerate with the `gen_protos.sh` command above. It pins `proto_version` to upstream `main`.
- Phase 1 moves protos to a root `proto/` dir. Not there yet.
