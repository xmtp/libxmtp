# xmtp_proto

Generated protobuf types and gRPC stubs.

## Commands

```bash
just check crate xmtp_proto
just lint-rust                          # workspace-wide. No per-crate lint.
just test crate xmtp_proto
just test workspace -p xmtp_proto --ignore-default-filter test_is_commit   # one test
dev/nix-shell "cargo nextest run --profile ci -p xmtp_proto -E 'test(/types::/)'"   # one module
dev/nix-shell 'buf lint proto'          # lint owned protobuf sources
```

## Gotchas

- `test-utils` is portable. Native proxy client helpers require `test-utils-network`.
- `types::ConversationType` is shared metadata. SQL conversions require `diesel`.
- Protobuf sources live in the root `proto/` directory.
- The build script generates Rust and serde code in Cargo `OUT_DIR`. Do not commit generated code.
- `grpc_client_impls` generates typed clients without transport constructors. Backend RPC tests enable it; normal consumers do not need it.

## Conventions

- Generated prost code is surfaced by `pub use generated::*` in `src/lib.rs` and the aliases `xmtp_proto::backend_v1` and `identity_v1`.
- Use the newtypes in `src/types/`, not `Vec<u8>` / `String`:
  - `types/ids/group_id.rs:22 GroupId`: `[u8; 16]`; `as_slice`, `as_bytes`, `into_bytes`, `to_vec`, `to_openmls`, `random(rand)`, `ZERO` / `ONE`.. `FOUR`, `Deref`, `FromStr` (error `GroupIdParseError:175`). Its Diesel `ToSql` / `FromSql<Binary, Sqlite>` needs the crate feature `diesel` (`group_id.rs:3`, `Cargo.toml:78`).
  - `types/ids/installation_id.rs:6 InstallationId`: `[u8; 32]` with a smaller API than `GroupId`: only `to_vec`, `Deref` / `AsRef`, `From<[u8; 32]>`, `Into<Vec<u8>>`, `TryFrom<Vec<u8>>` / `TryFrom<&[u8]>` (error `ConversionError`). No `as_bytes`, `into_bytes`, `to_openmls`, `random`, `FromStr`, or Diesel impl.
  - `types/topic.rs:Topic` / `TopicKind`: build with `Topic::new_group_message(..)`, `new_welcome_message(..)`, `new_identity_update(..)`, `new_key_package(..)`. Never concatenate topic bytes by hand.
  - Payload types include `GroupMessage`, `WelcomeMessage`, and `GroupMessageMetadata`. Backend decoders build these from `ServerEnvelope`.
  - `types/cursor.rs`: `Cursor(SequenceId)` is a scalar position on one topic. Zero starts at the beginning.
  - `TopicCursor` is a map from topic to cursor. `SequenceId` is `u64`.
- New newtype conversions: infallible `From` for fixed-size arrays (`From<[u8; 16]> for GroupId`), `TryFrom` for `Vec<u8>` / `&[u8]` with a typed error.
- Inbox ids are lowercase hex `String` (`crates/xmtp_common/src/types.rs:InboxId`). Normalize untrusted input with `crates/xmtp_common/src/hex.rs:NormalizeHex::normalize_hex` (lowercases, strips `0x`). Never hand-roll `to_lowercase().trim_start_matches("0x")`.

- `api::AuthError` owns the four public auth codes and stores retryability at creation.
  Keep it in `ApiClientError::Auth`; `Other` erases its code.
  `xmtp_api::dyn_err` maps it to `ApiError::Auth` before network error erasure.
