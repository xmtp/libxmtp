# xmtp_proto

Generated protobuf types and gRPC stubs.

## Commands

```bash
just check crate xmtp_proto
just test crate xmtp_proto
just test workspace -p xmtp_proto --ignore-default-filter test_is_commit   # one test
just test workspace -p xmtp_proto types::   # one module
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
  - `types/ids/group_id.rs GroupId`: 32 bytes (keyed: the group signing public key) or 16 bytes (legacy, imported archives only); `Copy`, and `Eq` / `Ord` / `Hash` follow the bytes as `[u8]` does. `as_slice`, `len`, `is_legacy`, `to_vec`, `to_openmls`, `random(rand)` (16 bytes until keyed creation lands), `ZERO` / `ONE`.. `FOUR` (16-byte), `Deref<Target = [u8]>`, `Borrow<[u8]>`, `FromStr` (error `GroupIdParseError`). `From<[u8; 16]>` and `From<[u8; 32]>`; every fallible conversion (`Vec<u8>`, `&[u8]`, OpenMLS, serde, hex, Diesel) accepts exactly 16 or 32 bytes. Its Diesel `ToSql` / `FromSql<Binary, Sqlite>` needs the crate feature `diesel`. Length constants: `xmtp_configuration::{GROUP_ID_BYTES, LEGACY_GROUP_ID_BYTES}`.
  - `types/ids/installation_id.rs:6 InstallationId`: `[u8; 32]` with a smaller API than `GroupId`: only `to_vec`, `Deref` / `AsRef`, `From<[u8; 32]>`, `Into<Vec<u8>>`, `TryFrom<Vec<u8>>` / `TryFrom<&[u8]>` (error `ConversionError`). No `as_bytes`, `into_bytes`, `to_openmls`, `random`, `FromStr`, or Diesel impl.
  - `types/topic.rs:Topic` / `TopicKind`: build with `Topic::new_group_message(..)`, `new_commit_log(..)`, `new_welcome_message(..)`, `new_identity_update(..)`, `new_key_package(..)`. Never concatenate topic bytes by hand. `Topic::parse` accepts a 32-byte or a 16-byte identifier for kinds 0 and 4 until the cutover to 32-byte identifiers. A group topic and a welcome topic can both be 33 bytes: branch on the kind byte, never on the length.
  - Payload types include `GroupMessage`, `WelcomeMessage`, and `GroupMessageMetadata`. Backend decoders build these from `ServerEnvelope`.
  - `types/cursor.rs`: `Cursor(SequenceId)` is a scalar position on one topic. Zero starts at the beginning.
  - `TopicCursor` is a map from topic to cursor. `SequenceId` is `u64`.
- New newtype conversions: infallible `From` for fixed-size arrays (`From<[u8; 32]> for GroupId`), `TryFrom` for `Vec<u8>` / `&[u8]` with a typed error.
- Inbox ids are lowercase hex `String` (`crates/xmtp_common/src/types.rs:InboxId`). Normalize untrusted input with `crates/xmtp_common/src/hex.rs:NormalizeHex::normalize_hex` (lowercases, strips `0x`). Never hand-roll `to_lowercase().trim_start_matches("0x")`.

- `api::AuthError` owns the four public auth codes and stores retryability at creation.
  Keep it in `ApiClientError::Auth`; `Other` erases its code.
  `xmtp_api::dyn_err` maps it to `ApiError::Auth` before network error erasure.
  `ExhaustedAfterAttempt` marks the failure that starts lockout. It has the
  same public code and message as `Exhausted`, which marks a later refusal.
