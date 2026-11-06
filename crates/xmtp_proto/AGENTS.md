# xmtp_proto

Generated protobuf/gRPC types. Schemas live in root `proto/`; build output goes
in `OUT_DIR`, never in commits. Run `dev/nix-shell 'just test crate xmtp_proto'`.

- Use `src/types` newtypes and `Topic` constructors, never assembled topic bytes.
  Fixed-size conversions use `From`; variable bytes use `TryFrom` with typed errors.
  Normalize inbox input with `NormalizeHex::normalize_hex`, never hand-rolled logic.
- Current group IDs are 32 bytes; 16-byte IDs are for imported legacy archives
  and cannot name group/commit-log topics. Before ID/topic changes, read
  [newtype guidance](../../.agents/skills/writing-rust/types.md).
- `Cursor` is a scalar per-topic position; `TopicCursor` maps topics to cursors.
- Keep `api::AuthError` in `ApiClientError::Auth`; `Other` erases its code.
  Preserve retryability and the distinction between attempt exhaustion and later refusal.
- `test-utils` is portable; native proxies need `test-utils-network`. SQL
  conversions need `diesel`; typed RPC clients need `grpc_client_impls`.
- Generation details: [README](README.md); schema constraints: [proto](../../proto/AGENTS.md).
