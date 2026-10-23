# Protobuf schemas

```sh
dev/nix-shell 'just lint-proto'
dev/nix-shell 'just check crate xmtp_proto'
dev/nix-shell 'dev/agent-run cargo check -p xmtp_proto --target wasm32-unknown-unknown'
```

Keep imports relative and builds deterministic; never fetch schemas at build time.
`google/` and `protoc-gen-openapiv2/` are pinned third-party sources: do not edit.
Use narrow Buf exceptions only for retained legacy schemas. Rust/descriptors are
build outputs, never committed.
