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

## Conventions

- Generated prost code lives under `src/gen/`, surfaced by `pub use generated::*` in `src/lib.rs` and the aliases `xmtp_proto::mls_v1` / `identity_v1`.
- Use the newtypes in `src/types/`, not `Vec<u8>` / `String`:
  - `types/ids/group_id.rs:22 GroupId`: `[u8; 16]`; `as_slice`, `as_bytes`, `into_bytes`, `to_vec`, `to_openmls`, `random(rand)`, `ZERO` / `ONE`.. `FOUR`, `Deref`, `FromStr` (error `GroupIdParseError:175`). Its Diesel `ToSql` / `FromSql<Binary, Sqlite>` needs the crate feature `diesel` (`group_id.rs:3`, `Cargo.toml:78`).
  - `types/ids/installation_id.rs:6 InstallationId`: `[u8; 32]` with a smaller API than `GroupId`: only `to_vec`, `Deref` / `AsRef`, `From<[u8; 32]>`, `Into<Vec<u8>>`, `TryFrom<Vec<u8>>` / `TryFrom<&[u8]>` (error `ConversionError`). No `as_bytes`, `into_bytes`, `to_openmls`, `random`, `FromStr`, or Diesel impl.
  - `types/topic.rs:Topic` / `TopicKind`: build with `Topic::new_group_message(..)`, `new_welcome_message(..)`, `new_identity_update(..)`, `new_key_package(..)`. Never concatenate topic bytes by hand.
  - Payload wrappers, each with a `derive_builder` `builder()`. Pass these between layers, not raw prost structs: `types/group_message.rs:11 GroupMessage` (`is_commit`), `types/welcome_message.rs:15 WelcomeMessage` (`as_v1`), `types/orphaned_envelope.rs:11 OrphanedEnvelope`, `types/message_metadata.rs:GroupMessageMetadata`, `types/cursor_list.rs:CursorList`.
  - `types/cursor.rs:20 Cursor`: prefer the named constructors to `Cursor::new`: `commit_log`, `v3_welcomes`, `v3_messages`, `installations`, `mls_commits`, `inbox_log` (`:33-73`) each pin the right originator id.
  - Also present: `types/{global_cursor,topic_cursor,app_version,api_identifier}.rs`; scalar aliases `types.rs:27 OriginatorId` (`u32`), `SequenceId` (`u64`).
- New newtype conversions: infallible `From` for fixed-size arrays (`From<[u8; 16]> for GroupId`), `TryFrom` for `Vec<u8>` / `&[u8]` with a typed error.
- Inbox ids are lowercase hex `String` (`crates/xmtp_common/src/types.rs:InboxId`). Normalize untrusted input with `crates/xmtp_common/src/hex.rs:NormalizeHex::normalize_hex` (lowercases, strips `0x`). Never hand-roll `to_lowercase().trim_start_matches("0x")`.
