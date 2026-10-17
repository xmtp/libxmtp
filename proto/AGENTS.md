# Protobuf schemas

Authoritative XMTP wire schemas. Rust code is generated in Cargo `OUT_DIR` by `crates/xmtp_proto/build.rs`.

## Commands

```bash
just lint-proto
just check crate xmtp_proto
dev/nix-shell 'dev/agent-run cargo check -p xmtp_proto --target wasm32-unknown-unknown'
```

## Rules

- Keep imports relative to this directory.
- Keep inputs deterministic. Do not fetch schemas during a build.
- Do not edit files under `google/` or `protoc-gen-openapiv2/`. They are pinned third-party sources.
- Add narrow Buf exceptions only for retained legacy schemas.
- Do not commit generated Rust or descriptor files.
