# xmtp-backend

Phase 1 hello-world binary for the self-hosted backend.

## Commands

```bash
dev/nix-shell 'cargo build --locked -p xmtp_backend'
just check crate xmtp_backend
dev/nix-shell "cargo nextest run --locked -p xmtp_backend -E 'test(startup_message_is_written)'"
just lint-rust
dev/nix-shell 'cargo run --locked -p xmtp_backend'
```

The binary prints one fixed startup message and exits. It has no server or storage.
