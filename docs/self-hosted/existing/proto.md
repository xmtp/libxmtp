<!-- markdownlint-configure-file { "MD024": { "siblings_only": true }, "MD029": false } -->

# XMTP Protobuf Catalog — Phase 0 Reference

Read-only survey of `/Users/nickmolnar/code/xmtp/proto` (the `xmtp/proto` repo) and of
how `/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto` consumes it.

Proto repo commit surveyed: `dedb87251f23bee8133154706afbc0aa1348210d`
(`2026-08-19`, "feat(mls/database): add expected_field_value to Up...").
This is the same revision libxmtp pins in
`/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/proto_version`.

All proto paths below are relative to `/Users/nickmolnar/code/xmtp/proto/proto/`
unless written in full.

---

## Table of Contents

1. [Inventory](#1-inventory)
   - [1.1 Classification legend](#11-classification-legend)
   - [1.2 Full file inventory](#12-full-file-inventory)
   - [1.3 Package-level summary and libxmtp usage](#13-package-level-summary-and-libxmtp-usage)
   - [1.4 Packages compiled but unused](#14-packages-compiled-but-unused)
2. [Field-level definitions for backend.proto dependencies](#2-field-level-definitions-for-backendproto-dependencies)
   - [2.1 Dependency closure of backend.proto](#21-dependency-closure-of-backendproto)
   - [2.2 xmtp.mls.api.v1 — GroupMessage / GroupMessageInput](#22-xmtpmlsapiv1--groupmessage--groupmessageinput)
   - [2.3 xmtp.mls.api.v1 — WelcomeMessage / WelcomeMessageInput / WelcomeMetadata](#23-xmtpmlsapiv1--welcomemessage--welcomemessageinput--welcomemetadata)
   - [2.4 xmtp.mls.api.v1 — key package messages](#24-xmtpmlsapiv1--key-package-messages)
   - [2.5 xmtp.mls.api.v1 — paging, query, subscribe](#25-xmtpmlsapiv1--paging-query-subscribe)
   - [2.6 xmtp.mls.api.v1 — XIP-83 Subscribe](#26-xmtpmlsapiv1--xip-83-subscribe)
   - [2.7 xmtp.mls.api.v1 — commit log RPC messages](#27-xmtpmlsapiv1--commit-log-rpc-messages)
   - [2.8 xmtp.mls.api.v1 — GetNewestGroupMessage](#28-xmtpmlsapiv1--getnewestgroupmessage)
   - [2.9 xmtp.mls.api.v1 — the MlsApi service](#29-xmtpmlsapiv1--the-mlsapi-service)
   - [2.10 xmtp.mls.message_contents — commit_log.proto](#210-xmtpmlsmessage_contents--commit_logproto)
   - [2.11 xmtp.mls.message_contents — welcome_pointer.proto](#211-xmtpmlsmessage_contents--welcome_pointerproto)
   - [2.12 xmtp.mls.message_contents — wrapper_encryption.proto](#212-xmtpmlsmessage_contents--wrapper_encryptionproto)
   - [2.13 xmtp.identity.associations — association.proto](#213-xmtpidentityassociations--associationproto)
   - [2.14 xmtp.identity.associations — signature.proto](#214-xmtpidentityassociations--signatureproto)
   - [2.15 xmtp.message_contents — public_key.proto](#215-xmtpmessage_contents--public_keyproto)
   - [2.16 xmtp.message_contents — signature.proto](#216-xmtpmessage_contents--signatureproto)
   - [2.17 xmtp.identity.api.v1 — the whole file](#217-xmtpidentityapiv1--the-whole-file)
   - [2.18 xmtp.identity — credential.proto](#218-xmtpidentity--credentialproto)
   - [2.19 xmtp.xmtpv4.envelopes — envelopes.proto](#219-xmtpxmtpv4envelopes--envelopesproto)
   - [2.20 xmtp.xmtpv4.envelopes — payer_report.proto](#220-xmtpxmtpv4envelopes--payer_reportproto)
   - [2.21 xmtp.xmtpv4.message_api — message_api.proto](#221-xmtpxmtpv4message_api--message_apiproto)
   - [2.22 xmtp.xmtpv4.message_api — service split files](#222-xmtpxmtpv4message_api--service-split-files)
   - [2.23 xmtp.xmtpv4.payer_api and gateway_api](#223-xmtpxmtpv4payer_api-and-gateway_api)
   - [2.24 xmtp.xmtpv4.metadata_api](#224-xmtpxmtpv4metadata_api)
   - [2.25 xmtp.message_api.v1 — v3/v2 MessageApi (legacy)](#225-xmtpmessage_apiv1--v3v2-messageapi-legacy)
   - [2.26 xmtp.mls_validation.v1](#226-xmtpmls_validationv1)
   - [2.27 xmtp.migration.api.v1](#227-xmtpmigrationapiv1)
3. [Field-format notes](#3-field-format-notes)
   - [3.1 Hex strings vs raw bytes](#31-hex-strings-vs-raw-bytes)
   - [3.2 Topic derivation](#32-topic-derivation)
   - [3.3 AuthenticatedData](#33-authenticateddata)
   - [3.4 Cursors](#34-cursors)
4. [Proto build tooling](#4-proto-build-tooling)
   - [4.1 Proto repo tooling](#41-proto-repo-tooling)
   - [4.2 libxmtp's xmtp_proto build](#42-libxmtps-xmtp_proto-build)
   - [4.3 Exact libxmtp files that reference the proto repo](#43-exact-libxmtp-files-that-reference-the-proto-repo)
   - [4.4 What Phase 1 needs to change](#44-what-phase-1-needs-to-change)
5. [Open questions and inconsistencies](#5-open-questions-and-inconsistencies)
6. [Review status](#review-status)

---

## 1. Inventory

### 1.1 Classification legend

| Code | Meaning |
| --- | --- |
| **(a)** | Needed by the new self-hosted backend API (`docs/self-hosted/backend.proto`) or in its transitive import closure |
| **(b)** | Client-only: content types, message contents, keystore, device sync, MLS wire formats. The backend never parses these |
| **(c)** | v4 / xmtpd-only (`xmtp.xmtpv4.*`) |
| **(d)** | v3-only or legacy (v2 `message_api.v1`, v2 `message_contents`, `keystore_api`) |
| **(e)** | `mls_validation` service protos |

A file can carry more than one code. `identity/associations/*` is (a) for the
backend and also (b) because the client builds and verifies the same structures
locally.

### 1.2 Full file inventory

Source: `find /Users/nickmolnar/code/xmtp/proto/proto -name '*.proto'` (62 files).

| # | File | Package | Class | One line |
| --- | --- | --- | --- | --- |
| 1 | `device_sync/consent_backup.proto` | `xmtp.device_sync.consent_backup` | (b) | Consent record shapes for device-sync/archive backups. |
| 2 | `device_sync/content.proto` | `xmtp.device_sync.content` | (b) | Sync-group message payloads (device sync request/reply). |
| 3 | `device_sync/device_sync.proto` | `xmtp.device_sync` | (b) | `BackupElement`, `BackupMetadataSave` — the archive envelope. |
| 4 | `device_sync/event_backup.proto` | `xmtp.device_sync.event_backup` | (b) | Local-event rows in a backup. |
| 5 | `device_sync/group_backup.proto` | `xmtp.device_sync.group_backup` | (b) | Group rows in a backup (`ConversationTypeSave`, `GroupMembershipStateSave`). |
| 6 | `device_sync/message_backup.proto` | `xmtp.device_sync.message_backup` | (b) | Message rows in a backup. |
| 7 | `identity/api/v1/identity.proto` | `xmtp.identity.api.v1` | **(a)** (d) | v3 `IdentityApi`: publish/get identity updates, `GetInboxIds`, `VerifySmartContractWalletSignatures`. |
| 8 | `identity/associations/association.proto` | `xmtp.identity.associations` | **(a)** (b) | `IdentityUpdate`, `IdentityAction`, `MemberIdentifier`, `AssociationState`. |
| 9 | `identity/associations/signature.proto` | `xmtp.identity.associations` | **(a)** (b) | Signature union: ERC-191, ERC-6492 SCW, Ed25519, legacy delegated, passkey. |
| 10 | `identity/credential.proto` | `xmtp.identity` | (b) (e) | `MlsCredential { inbox_id }` — the MLS leaf-node credential. |
| 11 | `keystore_api/v1/keystore.proto` | `xmtp.keystore_api.v1` | (d) | xmtp-js v2 keystore RPC surface. Dead weight for libxmtp. |
| 12 | `message_api/v1/authn.proto` | `xmtp.message_api.v1` | (d) | v2 auth `Token` / `AuthData` (wallet-addr bearer tokens). |
| 13 | `message_api/v1/message_api.proto` | `xmtp.message_api.v1` | (d) | v2 `MessageApi`: Publish/Subscribe/Query over string content topics. |
| 14 | `message_contents/ciphertext.proto` | `xmtp.message_contents` | (b) (d) | v2 `Ciphertext` union. |
| 15 | `message_contents/composite.proto` | `xmtp.message_contents` | (b) (d) | v2 composite content type. |
| 16 | `message_contents/contact.proto` | `xmtp.message_contents` | (b) (d) | v2 contact bundles. |
| 17 | `message_contents/content.proto` | `xmtp.message_contents` | (b) (d) | v2 `EncodedContent` / `ContentTypeId`. |
| 18 | `message_contents/conversation_reference.proto` | `xmtp.message_contents` | (b) (d) | v2 conversation reference. |
| 19 | `message_contents/ecies.proto` | `xmtp.message_contents` | (b) (d) | ECIES wrapper payload. |
| 20 | `message_contents/frames.proto` | `xmtp.message_contents` | (b) (d) | Frames-action signature payloads. |
| 21 | `message_contents/invitation.proto` | `xmtp.message_contents` | (b) (d) | v2 invitation. |
| 22 | `message_contents/message.proto` | `xmtp.message_contents` | (b) (d) | v2 message transport/storage. |
| 23 | `message_contents/private_key.proto` | `xmtp.message_contents` | (b) (d) | v2 private key bundles. Used by libxmtp tests to forge legacy keys. |
| 24 | `message_contents/private_preferences.proto` | `xmtp.message_contents` | (b) (d) | v2 private preferences. |
| 25 | `message_contents/public_key.proto` | `xmtp.message_contents` | **(a)** (b) (d) | `SignedPublicKey`, `UnsignedPublicKey`, legacy `PublicKey`. Reached by `LegacyDelegatedSignature`. |
| 26 | `message_contents/signature.proto` | `xmtp.message_contents` | **(a)** (b) (d) | v2 `Signature` (ECDSACompact / WalletECDSACompact). Reached by `public_key.proto` and by `mls.proto`'s `RevokeInstallationRequest`. |
| 27 | `message_contents/signed_payload.proto` | `xmtp.message_contents` | (b) (d) | Signed byte-array wrapper. |
| 28 | `migration/api/v1/migration.proto` | `xmtp.migration.api.v1` | (c) | `D14nMigrationApi.FetchD14nCutover` — v3→d14n cutover timestamp. |
| 29 | `mls/api/v1/mls.proto` | `xmtp.mls.api.v1` | **(a)** (d) | v3 `MlsApi` — the file backend.proto pulls `GroupMessageInput`, `WelcomeMessageInput`, `UploadKeyPackageRequest` from. |
| 30 | `mls/database/intents.proto` | `xmtp.mls.database` | (b) | Local intent rows stored in the libxmtp SQLite DB. Never sent over the wire. |
| 31 | `mls/database/task.proto` | `xmtp.mls.database` | (b) | Local task rows. |
| 32 | `mls/message_contents/commit_log.proto` | `xmtp.mls.message_contents` | **(a)** (b) | `CommitLogEntry`, `PlaintextCommitLogEntry`, `CommitResult`. |
| 33 | `mls/message_contents/component_permissions.proto` | `xmtp.mls.message_contents` | (b) | App-data component permissions. |
| 34 | `mls/message_contents/content.proto` | `xmtp.mls.message_contents` | (b) | v3 `EncodedContent`, `ContentTypeId`, `PlaintextEnvelope`. |
| 35 | `mls/message_contents/content_types/delete_message.proto` | `xmtp.mls.message_contents.content_types` | (b) | Delete-message content type. |
| 36 | `mls/message_contents/content_types/edit_message.proto` | `...content_types` | (b) | Edit-message content type. |
| 37 | `mls/message_contents/content_types/leave_request.proto` | `...content_types` | (b) | Leave-request content type. |
| 38 | `mls/message_contents/content_types/multi_remote_attachment.proto` | `...content_types` | (b) | Multi-remote-attachment content type. |
| 39 | `mls/message_contents/content_types/reaction.proto` | `...content_types` | (b) | Reaction content type. |
| 40 | `mls/message_contents/content_types/wallet_send_calls.proto` | `...content_types` | (b) | Wallet send-calls content type. |
| 41 | `mls/message_contents/external_commit_policy.proto` | `xmtp.mls.message_contents` | (b) | Group-wide external-commit policy component. |
| 42 | `mls/message_contents/external_invite.proto` | `xmtp.mls.message_contents` | (b) | QR/link external-commit invite payloads. |
| 43 | `mls/message_contents/group_membership.proto` | `xmtp.mls.message_contents` | (b) | `GroupMembership` MLS group-context extension. |
| 44 | `mls/message_contents/group_metadata.proto` | `xmtp.mls.message_contents` | (b) | Immutable group metadata extension. |
| 45 | `mls/message_contents/group_mutable_metadata.proto` | `xmtp.mls.message_contents` | (b) | Mutable group metadata extension. |
| 46 | `mls/message_contents/group_permissions.proto` | `xmtp.mls.message_contents` | (b) | Group permission policy extension. |
| 47 | `mls/message_contents/oneshot.proto` | `xmtp.mls.message_contents` | (b) | Out-of-band device-to-device signaling payloads. |
| 48 | `mls/message_contents/proposal_support.proto` | `xmtp.mls.message_contents` | (b) | Proposal-support extension data. |
| 49 | `mls/message_contents/transcript_messages.proto` | `xmtp.mls.message_contents` | (b) | Group-updated transcript messages. |
| 50 | `mls/message_contents/welcome_pointer.proto` | `xmtp.mls.message_contents` | **(a)** (b) | `WelcomePointer`, `WelcomePointerWrapperAlgorithm`, AEAD types. Imported by `mls.proto`. |
| 51 | `mls/message_contents/wrapper_encryption.proto` | `xmtp.mls.message_contents` | **(a)** (b) | `WelcomeWrapperAlgorithm`, `WelcomeWrapperEncryption`. Imported by `mls.proto`. |
| 52 | `mls_validation/v1/service.proto` | `xmtp.mls_validation.v1` | **(e)** | `ValidationApi` — the Go node's sidecar validation gRPC service, implemented in `apps/mls_validation_service`. |
| 53 | `xmtpv4/envelopes/envelopes.proto` | `xmtp.xmtpv4.envelopes` | (c) | `ClientEnvelope`, `AuthenticatedData`, `PayerEnvelope`, `OriginatorEnvelope`, `Cursor`. |
| 54 | `xmtpv4/envelopes/payer_report.proto` | `xmtp.xmtpv4.envelopes` | (c) | `PayerReport`, `PayerReportAttestation`, `NodeSignature`. |
| 55 | `xmtpv4/gateway_api/gateway_api.proto` | `xmtp.xmtpv4.gateway_api` | (c) | `GatewayApi` — client→gateway publish, replaces `PayerApi`. |
| 56 | `xmtpv4/message_api/message_api.proto` | `xmtp.xmtpv4.message_api` | (c) | All v4 request/response messages plus `ReplicationApi` (all rpcs deprecated except `SubscribeOriginators`). |
| 57 | `xmtpv4/message_api/misbehavior_api.proto` | `xmtp.xmtpv4.message_api` | (c) | Node misbehavior reporting/query. Generated and compiled, but it produces no file of its own: it shares the `xmtp.xmtpv4.message_api` package, so its symbols land in `crates/xmtp_proto/src/gen/xmtp.xmtpv4.message_api.rs` (from line 1431: `UnsignedMisbehaviorReport`, `MisbehaviorReport`, `SubmitMisbehaviorReportRequest`, …). No hand-written call sites. |
| 58 | `xmtpv4/message_api/notification_api.proto` | `xmtp.xmtpv4.message_api` | (c) | `NotificationApi.SubscribeAllEnvelopes` for push servers. |
| 59 | `xmtpv4/message_api/publish_api.proto` | `xmtp.xmtpv4.message_api` | (c) | `PublishApi` — gateway→node. |
| 60 | `xmtpv4/message_api/query_api.proto` | `xmtp.xmtpv4.message_api` | (c) | `QueryApi` — client→node query/subscribe, incl. XIP-83 `Subscribe`. |
| 61 | `xmtpv4/metadata_api/metadata_api.proto` | `xmtp.xmtpv4.metadata_api` | (c) | Sync cursor, version, payer-info metadata. |
| 62 | `xmtpv4/payer_api/payer_api.proto` | `xmtp.xmtpv4.payer_api` | (c) | Deprecated `PayerApi` (publish client envelopes, `GetNodes`). |

### 1.3 Package-level summary and libxmtp usage

`crates/xmtp_proto/src/gen/mod.rs` declares exactly which generated packages
compile into the crate. `xmtp.xmtpv4.gateway_api` is generated on disk
(`crates/xmtp_proto/src/gen/xmtp.xmtpv4.gateway_api.rs`, 10.5 KB) but is **not
`include!`d** by `mod.rs`, so it is dead in the build. `misbehavior_api.proto`
produces no separate file — it shares the `xmtp.xmtpv4.message_api` package, so
its messages land inside `xmtp.xmtpv4.message_api.rs`.

Usage counts below are grep hits for `xmtp_proto::xmtp::<pkg>` / `xmtp::<pkg>::`
across `.rs` files in the libxmtp workspace, excluding `target/` and
`crates/xmtp_proto/src/gen/`.

| Package | In `gen/mod.rs`? | Usage hits | Verdict |
| --- | --- | --- | --- |
| `xmtp.device_sync` (+5 sub-packages) | yes | 17 | **Used.** `xmtp_db`, `xmtp_archive` (`crates/xmtp_db/src/encrypted_store/group/convert.rs`, `crates/xmtp_archive/src/exporter.rs`). |
| `xmtp.identity` | yes | (part of 71) | **Used.** `MlsCredential`. |
| `xmtp.identity.api.v1` | yes | (part of 71) | **Used** heavily; re-exported as `xmtp_proto::identity_v1` (`crates/xmtp_proto/src/lib.rs`). |
| `xmtp.identity.associations` | yes | (part of 71) | **Used** heavily by `xmtp_id`. |
| `xmtp.keystore_api.v1` | yes | **0** | **Unused.** Only the generated file mentions it. 42 KB + 214 KB of serde. Drop. |
| `xmtp.message_api.v1` | yes | 3 + a public re-export | **Used, and not removable as-is.** `crates/xmtp_proto/src/api_client.rs:1-4` re-exports eight v2 types (`BatchQueryRequest`, `BatchQueryResponse`, `Envelope`, `PublishRequest`, `PublishResponse`, `QueryRequest`, `QueryResponse`, `SubscribeRequest`) from the crate root through `pub use generated::*` in `crates/xmtp_proto/src/lib.rs:22`. See §5-Q6. |
| `xmtp.message_contents` | yes | 7 | **Used narrowly** — `SignedPublicKey`, `Signature`, `signed_private_key`, `unsigned_public_key` for legacy-delegated identity and test fixtures. |
| `xmtp.migration.api.v1` | yes | 3 | **Used** by `xmtp_api_d14n` `FetchD14nCutover` and `apps/xnet`. Dies with d14n. |
| `xmtp.mls.api.v1` | yes | 204 (with the rest of `mls`) | **Used** heavily; re-exported as `xmtp_proto::mls_v1`. |
| `xmtp.mls.database` | yes | 40 | **Used** by `xmtp_mls` for local intent/task storage. |
| `xmtp.mls.message_contents` | yes | (part of 204) | **Used** heavily. |
| `xmtp.mls.message_contents.content_types` | yes | 204 (incl. bindings) | **Used** heavily by bindings. |
| `xmtp.mls_validation.v1` | yes | 103 in 9 files | **Used** by `apps/mls_validation_service` only (server side). |
| `xmtp.xmtpv4.envelopes` | yes | (part of 77) | **Used** by `xmtp_api_d14n`. |
| `xmtp.xmtpv4.message_api` | yes | (part of 77) | **Used** by `xmtp_api_d14n`. |
| `xmtp.xmtpv4.metadata_api` | yes | **0** outside `build.rs` and `gen/` | **Unused.** 22 KB + 34 KB serde. |
| `xmtp.xmtpv4.payer_api` | yes | (part of 77) | **Used** by `xmtp_api_d14n` publish path. |
| `xmtp.xmtpv4.gateway_api` | **no** | 2 (an unrelated local module named `gateway_api`) | **Generated but never compiled.** The two hits are `crates/xmtp_api_d14n/src/middleware/multi_node_client/gateway_api.rs`, a hand-written module, not the proto package. |

### 1.4 Packages compiled but unused

Confirmed dead in the libxmtp build, safe to leave out of the new in-repo
`proto/` folder unless something else pulls them in:

1. **`xmtp.keystore_api.v1`** — zero references. Only the v2 xmtp-js keystore.
2. **`xmtp.xmtpv4.metadata_api`** — zero references outside `build.rs`'s
   `server_mod_attribute` list and the generated files themselves.
3. **`xmtp.xmtpv4.gateway_api`** — generated but not in `gen/mod.rs`, so it does
   not even compile today.
4. **Most of `xmtp.message_contents`** — only 7 call sites, all for
   `SignedPublicKey` / `Signature` / legacy key fixtures. The other v2 files
   (composite, contact, content, conversation_reference, ecies, frames,
   invitation, message, private_preferences, signed_payload) have no Rust call
   sites at all — they compile purely because `build.rs` walks the whole proto
   tree.

   **`ciphertext.proto` is the exception: it is not independently removable.**
   `message_contents/private_key.proto:9` imports it, and `private_key.proto`
   is used by the legacy-key test fixture at
   `crates/xmtp_mls/src/test/builder.rs:25-30` (`SignedPrivateKey`,
   `signed_private_key::{Secp256k1, Union}`). Deleting `ciphertext.proto` alone
   breaks the `private_key.proto` compile. To drop it, first rewrite or remove
   the legacy-key fixture, then delete `private_key.proto` and
   `ciphertext.proto` together.
5. **`xmtp.message_api.v1` (v2 MessageApi)** — 3 direct hits, of which two are
   the `SortDirection` enum and one is a test. But the package also has a
   non-test public re-export: `crates/xmtp_proto/src/api_client.rs:1-4` does

   ```rust
   pub use super::xmtp::message_api::v1::{
       BatchQueryRequest, BatchQueryResponse, Envelope, PublishRequest, PublishResponse,
       QueryRequest, QueryResponse, SubscribeRequest,
   };
   ```

   Those eight types leave the crate through `pub use generated::*` in
   `crates/xmtp_proto/src/lib.rs:22` and are part of the public API. So
   **`message_api.proto` is not removable by fixing the two `SortDirection`
   references alone.** Before the package can go, three things must change: the
   eight-type re-export in `api_client.rs` must be migrated or deleted, the two
   `SortDirection` references must point at the `mls.api.v1` enum (§5-Q6), and
   the test import of `PublishRequest` must be updated. `authn.proto` (`Token`,
   `AuthData`) has zero hits and can go on its own.
6. **`xmtpv4/message_api/misbehavior_api.proto`** — its symbols *are* generated
   and compiled (they share the `xmtp.xmtpv4.message_api` output file, from
   `crates/xmtp_proto/src/gen/xmtp.xmtpv4.message_api.rs:1431`), but no
   hand-written code references them;
   `crates/xmtp_api_d14n/src/protocol/resolve.rs:6` only mentions "misbehavior
   report" in a comment. Dead by usage, not by generation.

The build compiles all of these because `crates/xmtp_proto/build.rs` uses
`WalkDir::new(out_dir.join("proto").join("proto"))` and feeds every `.proto` it
finds to `tonic-prost-build`. There is no allowlist.

---

## 2. Field-level definitions for backend.proto dependencies

### 2.1 Dependency closure of backend.proto

`/Users/nickmolnar/code/xmtp/libxmtp/docs/self-hosted/backend.proto` names five
external types in its `ClientEnvelope` oneof plus two in `IdentityService`:

| backend.proto reference | Defined in |
| --- | --- |
| `xmtp.mls.api.v1.GroupMessageInput` | `mls/api/v1/mls.proto` |
| `xmtp.mls.api.v1.WelcomeMessageInput` | `mls/api/v1/mls.proto` |
| `xmtp.mls.api.v1.UploadKeyPackageRequest` | `mls/api/v1/mls.proto` |
| `xmtp.identity.associations.IdentityUpdate` | `identity/associations/association.proto` |
| `xmtp.mls.message_contents.CommitLogEntry` | `mls/message_contents/commit_log.proto` |
| `GetInboxIdsRequest` / `GetInboxIdsResponse` | unqualified — see §5-Q1 |
| `VerifySmartContractWalletSignaturesRequest` / `...Response` | unqualified — `identity/api/v1/identity.proto` |

Transitive import closure, minimum set of files the new in-repo `proto/` folder
must carry for backend.proto to compile:

```text
mls/api/v1/mls.proto
├── identity/associations/signature.proto          (RecoverableEd25519Signature)
│   └── message_contents/public_key.proto          (SignedPublicKey, via LegacyDelegatedSignature)
│       └── message_contents/signature.proto
├── message_contents/signature.proto               (RevokeInstallationRequest.wallet_signature)
├── mls/message_contents/commit_log.proto
│   └── identity/associations/signature.proto
├── mls/message_contents/welcome_pointer.proto     (no imports)
├── mls/message_contents/wrapper_encryption.proto  (no imports)
├── google/api/annotations.proto                   (external, grpc-gateway)
├── google/protobuf/empty.proto                    (well-known)
└── protoc-gen-openapiv2/options/annotations.proto (external, grpc-gateway)

identity/associations/association.proto
└── identity/associations/signature.proto

identity/api/v1/identity.proto
├── identity/associations/association.proto
├── google/api/annotations.proto
└── protoc-gen-openapiv2/options/annotations.proto
```

That is **9 first-party files** for the backend surface — `mls/api/v1/mls.proto`,
`identity/associations/signature.proto`, `identity/associations/association.proto`,
`identity/api/v1/identity.proto`, `message_contents/public_key.proto`,
`message_contents/signature.proto`, `mls/message_contents/commit_log.proto`,
`mls/message_contents/welcome_pointer.proto`,
`mls/message_contents/wrapper_encryption.proto` — plus the two
grpc-gateway/googleapis externals which exist only for HTTP annotations and can
be dropped if the new backend is gRPC-only (see §5-Q10).

### 2.2 `xmtp.mls.api.v1` — GroupMessage / GroupMessageInput

`mls/api/v1/mls.proto:217-247`:

```proto
// Full representation of a group message
message GroupMessage {
  // Version 1 of the GroupMessage format
  message V1 {
    uint64 id = 1;
    uint64 created_ns = 2;
    bytes group_id = 3;
    bytes data = 4;
    bytes sender_hmac = 5;
    bool should_push = 6;
    bool is_commit = 7;
  }

  oneof version {
    V1 v1 = 1;
  }
}

// Input type for a group message
message GroupMessageInput {
  // Version 1 of the GroupMessageInput payload format
  message V1 {
    bytes data = 1; // Serialized MlsProtocolMessage
    bytes sender_hmac = 2;
    bool should_push = 3;
  }

  oneof version {
    V1 v1 = 1;
  }
}
```

Note the asymmetry: `GroupMessage.V1` carries `group_id` and `is_commit` as
explicit fields, while `GroupMessageInput.V1` carries neither. The server
derives both by TLS-deserializing `data` as an MLS `ProtocolMessage` — libxmtp
does the same in
`/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_api_d14n/src/protocol/extractors/topics.rs:86-92`.

`mls/api/v1/mls.proto:249-257`:

```proto
// Send a batch of MLS messages
message SendGroupMessagesRequest {
  repeated GroupMessageInput messages = 1;
}

// Send a batch of welcome messages
message SendWelcomeMessagesRequest {
  repeated WelcomeMessageInput messages = 1;
}
```

### 2.3 `xmtp.mls.api.v1` — WelcomeMessage / WelcomeMessageInput / WelcomeMetadata

`mls/api/v1/mls.proto:142-215`:

```proto
// Full representation of a welcome message
message WelcomeMessage {
  // Version 1 of the WelcomeMessage format
  message V1 {
    uint64 id = 1;
    uint64 created_ns = 2;
    bytes installation_key = 3;
    bytes data = 4;
    bytes hpke_public_key = 5;
    xmtp.mls.message_contents.WelcomeWrapperAlgorithm wrapper_algorithm = 6;
    bytes welcome_metadata = 7;
  }

  message WelcomePointer {
    uint64 id = 1;
    uint64 created_ns = 2;
    // The topic of the welcome message (generally the installation id)
    bytes installation_key = 3;
    // A WelcomePointer encrypted using the algorithm specified by
    // wrapper_algorithm
    bytes welcome_pointer = 4;
    // The public key used to encrypt the welcome pointer
    bytes hpke_public_key = 5;
    // The algorithm used to encrypt the welcome pointer
    xmtp.mls.message_contents.WelcomePointerWrapperAlgorithm wrapper_algorithm = 6;
  }

  oneof version {
    V1 v1 = 1;
    WelcomePointer welcome_pointer = 2;
  }
}

// Input type for a welcome message
message WelcomeMessageInput {
  // Version 1 of the WelcomeMessageInput format, if used as the pointee of a
  // WelcomePointer then the hpke_public_key will be unset, and the
  // wrapper_algorithm will be WELCOME_WRAPPER_ALGORITHM_SYMMETRIC_KEY
  message V1 {
    // The topic of the welcome message (generally the installation id)
    bytes installation_key = 1;
    // An encrypted mls `Welcome` struct
    bytes data = 2;
    // The public key of the welcome message
    bytes hpke_public_key = 3;
    // The algorithm used to encrypt the welcome message
    xmtp.mls.message_contents.WelcomeWrapperAlgorithm wrapper_algorithm = 4;
    // The metadata of the welcome message
    bytes welcome_metadata = 7;
  }

  // Version 2 of the WelcomeMessageInput format which uses a WelcomePointer
  // to point to the welcome message for several installations at once
  message WelcomePointer {
    // The topic of the welcome message (generally the installation id)
    bytes installation_key = 1;
    // A WelcomePointer encrypted using the wrapper_algorithm
    bytes welcome_pointer = 2;
    // The public key used to encrypt the welcome pointer
    bytes hpke_public_key = 3;
    // The algorithm used to encrypt the welcome pointer
    xmtp.mls.message_contents.WelcomePointerWrapperAlgorithm wrapper_algorithm = 4;
  }

  oneof version {
    V1 v1 = 1;
    WelcomePointer welcome_pointer = 2;
  }
}

// This field is encrypted along with the `data` field on the welcome message.
message WelcomeMetadata {
  uint64 message_cursor = 1;
}
```

`WelcomeMessageInput.V1.welcome_metadata` is field **7**, not 5. Fields 5 and 6
were skipped/reserved-by-omission — the file has no `reserved` statement for
them. See §5-Q7.

### 2.4 `xmtp.mls.api.v1` — key package messages

`mls/api/v1/mls.proto:259-307`:

```proto
// A wrapper around the Key Package bytes
message KeyPackageUpload {
  // This would be a serialized MLS key package that the node would
  // parse, validate, and then store.

  // The owner's wallet address would be extracted from the identity
  // credential in the key package, and all signatures would be validated.
  bytes key_package_tls_serialized = 1;
}

// Register a new installation
message RegisterInstallationRequest {
  // The Key Package contains all information needed to register an installation
  KeyPackageUpload key_package = 1;
  bool is_inbox_id_credential = 2;
}

// The response to a RegisterInstallationRequest
message RegisterInstallationResponse {
  bytes installation_key = 1;
}

// Upload a new key packages
message UploadKeyPackageRequest {
  // An individual key package upload request
  KeyPackageUpload key_package = 1;
  bool is_inbox_id_credential = 2;
}

// Fetch one or more key packages
message FetchKeyPackagesRequest {
  // The caller can provide an array of installation keys, and the API
  // will return one key package for each installation associated with each
  // installation key
  repeated bytes installation_keys = 1;
}

// The response to a FetchKeyPackagesRequest
message FetchKeyPackagesResponse {
  // An individual key package
  message KeyPackage {
    bytes key_package_tls_serialized = 1;
  }

  // Returns one key package per installation in the original order of the
  // request. If any installations are missing key packages, an empty entry is
  // left in their respective spots in the array.
  repeated KeyPackage key_packages = 1;
}

// Revoke an installation
message RevokeInstallationRequest {
  bytes installation_key = 1;
  // All revocations must be validated with a wallet signature over the
  // installation_id being revoked (and some sort of standard prologue)
  xmtp.message_contents.Signature wallet_signature = 2;
}
```

`RevokeInstallationRequest` is the only reason `mls.proto` imports the v2
`message_contents/signature.proto`. libxmtp has zero call sites for
`RevokeInstallation`; the new backend can drop the rpc and the import together.

`mls/api/v1/mls.proto:317-353` (the v3 `GetIdentityUpdates`, superseded by
`identity.api.v1`; kept for the wallet-address era):

```proto
// Get all updates for an identity since the specified time
message GetIdentityUpdatesRequest {
  repeated string account_addresses = 1;
  uint64 start_time_ns = 2;
}

// Used to get any new or revoked installations for a list of wallet addresses
message GetIdentityUpdatesResponse {
  // A new installation key was seen for the first time by the nodes
  message NewInstallationUpdate {
    bytes installation_key = 1;
    bytes credential_identity = 2;
  }

  // An installation was revoked
  message RevokedInstallationUpdate {
    bytes installation_key = 1;
  }

  // A wrapper for any update to the wallet
  message Update {
    uint64 timestamp_ns = 1;
    oneof kind {
      NewInstallationUpdate new_installation = 2;
      RevokedInstallationUpdate revoked_installation = 3;
    }
  }

  // A wrapper for the updates for a single wallet
  message WalletUpdates {
    repeated Update updates = 1;
  }

  // A list of updates (or empty objects if no changes) in the original order
  // of the request
  repeated WalletUpdates updates = 1;
}
```

### 2.5 `xmtp.mls.api.v1` — paging, query, subscribe

`mls/api/v1/mls.proto:355-411`:

```proto
// Sort direction for queries
enum SortDirection {
  SORT_DIRECTION_UNSPECIFIED = 0;
  SORT_DIRECTION_ASCENDING = 1;
  SORT_DIRECTION_DESCENDING = 2;
}

// Pagination config for queries
message PagingInfo {
  SortDirection direction = 1;
  uint32 limit = 2;
  uint64 id_cursor = 3;
}

// Request for group message queries
message QueryGroupMessagesRequest {
  bytes group_id = 1;
  PagingInfo paging_info = 2;
}

// Response for group message queries
message QueryGroupMessagesResponse {
  repeated GroupMessage messages = 1;
  PagingInfo paging_info = 2;
}

// Request for welcome message queries
message QueryWelcomeMessagesRequest {
  bytes installation_key = 1;
  PagingInfo paging_info = 2;
}

// Response for welcome message queries
message QueryWelcomeMessagesResponse {
  repeated WelcomeMessage messages = 1;
  PagingInfo paging_info = 2;
}

// Request for subscribing to group messages
message SubscribeGroupMessagesRequest {
  // Subscription filter
  message Filter {
    bytes group_id = 1;
    uint64 id_cursor = 2;
  }
  repeated Filter filters = 1;
}

// Request for subscribing to welcome messages
message SubscribeWelcomeMessagesRequest {
  // Subscription filter
  message Filter {
    bytes installation_key = 1;
    uint64 id_cursor = 2;
  }
  repeated Filter filters = 1;
}
```

**There is no `Cursor` message in `xmtp.mls.api.v1`.** The v3 cursor is the bare
`uint64 id_cursor` inside `PagingInfo` / `Filter`. The named `Cursor` types are
`xmtp.message_api.v1.Cursor` (v2, an `IndexCursor` oneof — §2.25) and
`xmtp.xmtpv4.envelopes.Cursor` (v4, a `map<uint32,uint64>` vector clock — §2.19).
backend.proto defines a third, `xmtp.backend.v1.Cursor { uint64 sequence_id }`.

### 2.6 `xmtp.mls.api.v1` — XIP-83 Subscribe

`mls/api/v1/mls.proto:413-572`. This is the v3 binding of the same design
backend.proto's `SubscriptionService.Subscribe` copies. Comments are preserved
verbatim because they carry the delivery-ordering contract.

```proto
// --- XIP-83: bidirectional mutable subscription with liveness ----------------
//
// A single long-lived `Subscribe` stream replaces repeated server-streaming
// subscriptions. The client mutates its subscribed set in place (no reconnect on
// membership change), and the two sides exchange a WebSocket-style ping/pong so
// the client detects silent stream death and the node reaps a vanished peer.
//
// Subscriptions are expressed as kind-prefixed binary topics (XIP-49 §3.3.2):
// a leading kind byte plus the identifier, the same representation the
// decentralized backend uses, so one topic format spans both backends and a
// single stream can carry several topic kinds (group messages, welcomes).
// Request and response are wrapped in `oneof version` (cf. GroupMessage /
// WelcomeMessage) to leave room for future revisions. The versions are pinned
// per stream: a stream whose requests are V1 receives only V1 responses, so a
// client never has to handle a response version it did not speak first.

// Client -> server. Sent one or more times over the life of the stream.
message SubscribeRequest {
  oneof version {
    V1 v1 = 1;
  }

  message V1 {
    // Each frame is exactly one of: a mutation, a Ping, or a Pong.
    oneof request {
      Mutate mutate = 1;
      Ping ping = 2; // liveness challenge (e.g. probe the link after resuming)
      Pong pong = 3; // answer to a server Ping
    }

    // Add and/or remove subscriptions in place (applied atomically per frame).
    // Topics use the kind-prefixed binary representation shared with the
    // decentralized backend (XIP-49 §3.3.2): the first byte is the topic kind,
    // the remainder is the identifier. This RPC initially serves
    // TOPIC_KIND_GROUP_MESSAGES_V1 (0x00, identifier = group_id) and
    // TOPIC_KIND_WELCOME_MESSAGES_V1 (0x01, identifier = installation_key);
    // a topic whose kind the node does not serve fails the stream with
    // INVALID_ARGUMENT. Future kinds (key packages, identity updates) are
    // adopted via the capabilities advertised on Started.
    message Mutate {
      repeated Subscription adds = 1; // begin delivering these topics
      repeated bytes removes = 2; // stop delivering; clears the topic's cursor floor so a re-add replays

      // Catch this Mutate's adds up — history, TopicsLive markers, and the
      // wave's CatchupComplete — but do NOT register them for live delivery.
      // The markers then mean "you have everything as of the wave's start";
      // later messages arrive on no lane of this stream. Combined with
      // half-closing the request stream, this is the
      // bounded catch-up ("sync") mode: the server finishes the wave and then
      // closes the stream itself. Removals in the Mutate are unaffected.
      bool history_only = 3;

      // Client-chosen correlation id: echoed on this wave's CatchupComplete,
      // and stamped on every delivery frame of the wave's catch-up replay
      // (Messages.mutate_id). MUST be nonzero when adds are present (0 is the
      // live tag), and MUST NOT match the mutate_id of a wave still in flight
      // on the stream (an in-flight collision would make two waves' frames and
      // completions indistinguishable) — either violation fails the stream
      // with INVALID_ARGUMENT. SHOULD be unique per stream so completed waves
      // stay attributable too.
      uint64 mutate_id = 4;

      // A topic to subscribe, with the cursor to resume from.
      message Subscription {
        bytes topic = 1;
        // Deliver ids greater than this; 0 = from the beginning. For a newly
        // joined group, a client SHOULD seed this from the welcome's encrypted
        // WelcomeMetadata.message_cursor so a new membership does not refetch
        // pre-join history it cannot decrypt; for a new installation's welcome
        // topic, 0 is how pending welcomes are collected.
        uint64 id_cursor = 2;
      }
    }
  }
}

// Server -> client.
message SubscribeResponse {
  oneof version {
    V1 v1 = 1;
  }

  message V1 {
    oneof response {
      Messages messages = 1;
      Started started = 2; // sent once, immediately on open, before any catch-up
      Ping ping = 3; // idle liveness challenge; receiver MUST answer with Pong
      Pong pong = 4; // answer to a client Ping
      TopicsLive topics_live = 5; // no more replay for these topics; live begins after CatchupComplete
      CatchupComplete catchup_complete = 6; // acks a Mutate; wave completion if it started one
    }

    // A batch of new messages; group and welcome messages share the stream,
    // depending on which subscriptions are active. A frame belongs to exactly
    // one catch-up wave or to live — the server never mixes lanes, or two
    // waves, in one frame — and each lane is delivered in ascending id order
    // per message kind (live: across all live topics on the stream; a wave:
    // across the wave's topics, one merged cursor-ordered pass).
    message Messages {
      repeated GroupMessage group_messages = 1;
      repeated WelcomeMessage welcome_messages = 2;
      // The catch-up wave that produced this frame: the Mutate's mutate_id
      // for wave replay, 0 for live tail.
      uint64 mutate_id = 3;
    }

    // The first frame on every stream.
    message Started {
      // The server's ping cadence (ms): the basis for the client's staleness
      // threshold and the server's reap deadline.
      uint32 keepalive_interval_ms = 1;
      // Optional protocol features the node supports on this stream. The node
      // silently ignores request types it does not understand, so a client
      // MUST NOT send an optional request type whose capability the node did
      // not advertise (it would hang waiting on a response that never comes).
      repeated Capability capabilities = 2;
    }

    // Sent once per Mutate: at wave completion (after the wave's last
    // TopicsLive) for a Mutate that started a catch-up "wave", immediately for
    // one that did not (nothing added — removes-only or empty — or every add
    // a no-op). Also the catch-up
    // seam: live frames (mutate_id 0) for the wave's topics begin only after
    // this frame.
    message CatchupComplete {
      uint64 mutate_id = 1; // echoes the Mutate; 0 only if a waveless Mutate carried 0
    }

    // Emitted when topics finish catch-up, AFTER the last history frame for
    // them — including messages that arrived mid-wave and were folded into it,
    // which were equally historical from the client's perspective — so no
    // further replay for a listed topic follows; its live (mutate_id 0) frames
    // begin after the wave's CatchupComplete. Informational only: delivery
    // correctness (no duplicates, no gaps) never depends on it. Re-adding a
    // topic re-runs catch-up and re-emits it; receivers treat it idempotently.
    message TopicsLive {
      repeated bytes topics = 1; // kind-prefixed topics done replaying
    }

    // Optional per-stream protocol features (none defined yet; future
    // revisions add values, e.g. fetch-over-stream lookups answered with the
    // same read view that feeds the stream, or new streamable topic kinds).
    enum Capability {
      CAPABILITY_UNSPECIFIED = 0;
    }
  }
}

// Liveness challenge/response, shared across versions. Either peer MAY send a
// Ping; the receiver MUST reply with a Pong echoing the nonce. The sender closes
// the stream if no Pong arrives within its deadline — how a node reaps a vanished
// peer (e.g. a mobile client the OS suspended behind a proxy that still ACKs the
// transport).
message Ping {
  uint64 nonce = 1;
}

message Pong {
  uint64 nonce = 1; // echoes the nonce of the Ping it answers
}
```

### 2.7 `xmtp.mls.api.v1` — commit log RPC messages

`mls/api/v1/mls.proto:574-601`:

```proto
message BatchPublishCommitLogRequest {
  repeated PublishCommitLogRequest requests = 1;
}

message PublishCommitLogRequest {
  bytes group_id = 1;
  bytes serialized_commit_log_entry = 2;
  xmtp.identity.associations.RecoverableEd25519Signature signature = 3;
}

message QueryCommitLogRequest {
  bytes group_id = 1;
  PagingInfo paging_info = 2;
}

message QueryCommitLogResponse {
  bytes group_id = 1;
  repeated xmtp.mls.message_contents.CommitLogEntry commit_log_entries = 2;
  PagingInfo paging_info = 3;
}

message BatchQueryCommitLogRequest {
  repeated QueryCommitLogRequest requests = 1;
}

message BatchQueryCommitLogResponse {
  repeated QueryCommitLogResponse responses = 1;
}
```

`PublishCommitLogRequest` and `CommitLogEntry` are near-duplicates: publish adds
`group_id`, entry adds `sequence_id`. See §5-Q4.

### 2.8 `xmtp.mls.api.v1` — GetNewestGroupMessage

`mls/api/v1/mls.proto:603-619`:

```proto
// Request to get the newest group message from a range of topics
message GetNewestGroupMessageRequest {
  // Get the newest message from each of these topics
  repeated bytes group_ids = 1;
  bool include_content = 2;
}

// Returns a list of responses that will always be the same length as the
// request
message GetNewestGroupMessageResponse {
  message Response {
    // If no message is found on the topic, will be nil
    optional GroupMessage group_message = 1;
  }

  repeated Response responses = 1;
}
```

backend.proto's `QueryNewest` generalizes this to arbitrary topics and renames
`include_content` to `include_full_envelope`.

### 2.9 `xmtp.mls.api.v1` — the MlsApi service

`mls/api/v1/mls.proto:1-140`. Header and full service, with HTTP annotations:

```proto
// Message API
syntax = "proto3";
package xmtp.mls.api.v1;

import "google/api/annotations.proto";
import "google/protobuf/empty.proto";
import "identity/associations/signature.proto";
import "message_contents/signature.proto";
import "mls/message_contents/commit_log.proto";
import "mls/message_contents/welcome_pointer.proto";
import "mls/message_contents/wrapper_encryption.proto";
import "protoc-gen-openapiv2/options/annotations.proto";

option go_package = "github.com/xmtp/proto/v3/go/mls/api/v1";
option java_package = "org.xmtp.proto.mls.api.v1";
option (grpc.gateway.protoc_gen_openapiv2.options.openapiv2_swagger) = {
  info: {
    title: "MlsApi"
    version: "1.0"
  }
};

// RPCs for the new MLS API
service MlsApi {
  // Send a MLS payload, that would be validated before being stored to the
  // network
  rpc SendGroupMessages(SendGroupMessagesRequest) returns (google.protobuf.Empty) {
    option (google.api.http) = {
      post: "/mls/v1/send-group-messages"
      body: "*"
    };
  }

  // Send a batch of welcome messages
  rpc SendWelcomeMessages(SendWelcomeMessagesRequest) returns (google.protobuf.Empty) {
    option (google.api.http) = {
      post: "/mls/v1/send-welcome-messages"
      body: "*"
    };
  }

  // Register a new installation, which would be validated before storage
  rpc RegisterInstallation(RegisterInstallationRequest) returns (RegisterInstallationResponse) {
    option (google.api.http) = {
      post: "/mls/v1/register-installation"
      body: "*"
    };
  }

  // Upload a new KeyPackage, which would be validated before storage
  rpc UploadKeyPackage(UploadKeyPackageRequest) returns (google.protobuf.Empty) {
    option (google.api.http) = {
      post: "/mls/v1/upload-key-package"
      body: "*"
    };
  }

  // Get one or more Key Packages by installation_id
  rpc FetchKeyPackages(FetchKeyPackagesRequest) returns (FetchKeyPackagesResponse) {
    option (google.api.http) = {
      post: "/mls/v1/fetch-key-packages"
      body: "*"
    };
  }

  // Would delete all key packages associated with the installation and mark
  // the installation as having been revoked
  rpc RevokeInstallation(RevokeInstallationRequest) returns (google.protobuf.Empty) {
    option (google.api.http) = {
      post: "/mls/v1/revoke-installation"
      body: "*"
    };
  }

  // Used to check for changes related to members of a group.
  // Would return an array of any new installations associated with the wallet
  // address, and any revocations that have happened.
  rpc GetIdentityUpdates(GetIdentityUpdatesRequest) returns (GetIdentityUpdatesResponse) {
    option (google.api.http) = {
      post: "/mls/v1/get-identity-updates"
      body: "*"
    };
  }

  // Query stored group messages
  rpc QueryGroupMessages(QueryGroupMessagesRequest) returns (QueryGroupMessagesResponse) {
    option (google.api.http) = {
      post: "/mls/v1/query-group-messages"
      body: "*"
    };
  }

  // Query stored group messages
  rpc QueryWelcomeMessages(QueryWelcomeMessagesRequest) returns (QueryWelcomeMessagesResponse) {
    option (google.api.http) = {
      post: "/mls/v1/query-welcome-messages"
      body: "*"
    };
  }

  // Subscribe stream of new group messages
  rpc SubscribeGroupMessages(SubscribeGroupMessagesRequest) returns (stream GroupMessage) {
    option (google.api.http) = {
      post: "/mls/v1/subscribe-group-messages"
      body: "*"
    };
  }

  // Subscribe stream of new welcome messages
  rpc SubscribeWelcomeMessages(SubscribeWelcomeMessagesRequest) returns (stream WelcomeMessage) {
    option (google.api.http) = {
      post: "/mls/v1/subscribe-welcome-messages"
      body: "*"
    };
  }

  // Bidirectional subscription (XIP-83). One long-lived stream the client mutates
  // in place via add/remove topic deltas, with WebSocket-style liveness ping/pong.
  // A single stream MAY carry both group-message and welcome topics.
  // gRPC-only: bidirectional streaming has no HTTP/grpc-gateway mapping.
  rpc Subscribe(stream SubscribeRequest) returns (stream SubscribeResponse) {}

  rpc BatchPublishCommitLog(BatchPublishCommitLogRequest) returns (google.protobuf.Empty) {
    option (google.api.http) = {
      post: "/mls/v1/batch-publish-commit-log"
      body: "*"
    };
  }

  rpc BatchQueryCommitLog(BatchQueryCommitLogRequest) returns (BatchQueryCommitLogResponse) {
    option (google.api.http) = {
      post: "/mls/v1/batch-query-commit-log"
      body: "*"
    };
  }

  rpc GetNewestGroupMessage(GetNewestGroupMessageRequest) returns (GetNewestGroupMessageResponse) {
    option (google.api.http) = {get: "/mls/v1/get-newest-group-message"};
  }
}
```

**Mapping to the new backend, 15 rpcs → 7:**

| v3 MlsApi rpc | backend.proto equivalent |
| --- | --- |
| `SendGroupMessages` | `PublishService.Publish` (`ClientEnvelope.group_message`) |
| `SendWelcomeMessages` | `PublishService.Publish` (`ClientEnvelope.welcome_message`) |
| `UploadKeyPackage` | `PublishService.Publish` (`ClientEnvelope.key_package`) |
| `BatchPublishCommitLog` | `PublishService.Publish` (`ClientEnvelope.commit_log_entry`) |
| `QueryGroupMessages`, `QueryWelcomeMessages`, `BatchQueryCommitLog` | `QueryService.Query` (topic-scoped) |
| `GetNewestGroupMessage` | `QueryService.QueryNewest` |
| `SubscribeGroupMessages`, `SubscribeWelcomeMessages`, `Subscribe` | `SubscriptionService.Subscribe` / `SubscribeOnce` |
| `RegisterInstallation` | **no equivalent** — see §5-Q3 |
| `RevokeInstallation` | **no equivalent** |
| `FetchKeyPackages` | **no equivalent** — see §5-Q3 |
| `GetIdentityUpdates` (mls v3, wallet-address) | dropped |

### 2.10 `xmtp.mls.message_contents` — commit_log.proto

`mls/message_contents/commit_log.proto` in full:

```proto
// Defines entries on the commit log, used for fork detection and recovery
// XIP: https://community.xmtp.org/t/xip-68-draft-automated-fork-recovery/951
syntax = "proto3";

package xmtp.mls.message_contents;

import "identity/associations/signature.proto";

enum CommitResult {
  COMMIT_RESULT_UNSPECIFIED = 0;
  COMMIT_RESULT_APPLIED = 1;
  COMMIT_RESULT_WRONG_EPOCH = 2;
  COMMIT_RESULT_UNDECRYPTABLE = 3;
  COMMIT_RESULT_INVALID = 4;
}

// PlaintextCommitLogEntry indicates whether a commit was successful or not,
// when applied on top of the indicated `last_epoch_authenticator`.
message PlaintextCommitLogEntry {
  // The group_id of the group that the commit belongs to.
  bytes group_id = 1;
  // The sequence ID of the commit payload being validated.
  uint64 commit_sequence_id = 2;
  // The encryption state before the commit was applied.
  bytes last_epoch_authenticator = 3;
  // Indicates whether the commit was successful, or why it failed.
  CommitResult commit_result = 4;
  // The epoch number after the commit was applied, if successful.
  uint64 applied_epoch_number = 5;
  // The encryption state after the commit was applied, if successful.
  bytes applied_epoch_authenticator = 6;
}

message CommitLogEntry {
  uint64 sequence_id = 1;
  bytes serialized_commit_log_entry = 2;
  xmtp.identity.associations.RecoverableEd25519Signature signature = 3;
}
```

This file has **no `option go_package` and no `option java_package`**, unlike
every other file in the repo. It is also the only imported dependency of
`mls.proto` in the `mls/message_contents` directory that carries no options.

The proto does not say what `serialized_commit_log_entry` holds. **In
production it is always a plaintext, unencrypted `PlaintextCommitLogEntry`.**
libxmtp encodes it with `entry.encode_to_vec()` at
`/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_mls/src/groups/commit_log.rs:393`
— no encryption step — and decodes it with `PlaintextCommitLogEntry::decode(...)`
at the same file, line 499. An encrypted "private commit log" variant is
discussed in the XIP but has no code path today.

This matters for routing: because the blob is plaintext and
`PlaintextCommitLogEntry.group_id` is its first field, a server can recover the
group id from a `CommitLogEntry` with one protobuf decode. See §5-Q4.

### 2.11 `xmtp.mls.message_contents` — welcome_pointer.proto

```proto
// WelcomePointer is used to point to the welcome message for several installations at once to save overhead
syntax = "proto3";

package xmtp.mls.message_contents;

option go_package = "github.com/xmtp/proto/v3/go/mls/message_contents";
option java_package = "org.xmtp.proto.mls.message.contents";

// A WelcomePointer is used to point to the welcome message for several installations at once to save overhead
message WelcomePointer {
  // Points to a V1 WelcomeMessage
  message WelcomeV1Pointer {
    // The topic of the welcome message. For V1, this means that it will be the first message in the topic, so no other identifier is required
    bytes destination = 1;
    // The algorithm used to encrypt the welcome pointer
    WelcomePointeeEncryptionAeadType aead_type = 2;
    // The encryption key of the welcome message. Must match key size specified by the aead_type.
    bytes encryption_key = 3;
    // Nonce used to encrypt the data field. Must match nonce size specified by the aead_type.
    bytes data_nonce = 4;
    // Nonce used to encrypt the welcome_metadata field. Must match nonce size specified by the aead_type.
    bytes welcome_metadata_nonce = 5;
  }

  oneof version {
    WelcomeV1Pointer welcome_v1_pointer = 1;
  }
}

enum WelcomePointeeEncryptionAeadType {
  WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_UNSPECIFIED = 0;
  // Use same encoding as openmls::AeadType
  WELCOME_POINTEE_ENCRYPTION_AEAD_TYPE_CHACHA20_POLY1305 = 3;
}

// MUST match the WelcomeWrapperAlgorithm enum values without 25519 so that the i32 transformations are compatible
enum WelcomePointerWrapperAlgorithm {
  WELCOME_POINTER_WRAPPER_ALGORITHM_UNSPECIFIED = 0;
  WELCOME_POINTER_WRAPPER_ALGORITHM_XWING_MLKEM_768_DRAFT_6 = 2;
}

// Extension message that indicates the types of encryption supported by a client
message WelcomePointeeEncryptionAeadTypesExtension {
  repeated WelcomePointeeEncryptionAeadType supported_aead_types = 1;
}
```

Two enums carry deliberate numbering holes: `WelcomePointeeEncryptionAeadType`
jumps 0 → 3 to match `openmls::AeadType`, and `WelcomePointerWrapperAlgorithm`
jumps 0 → 2 to stay `i32`-compatible with `WelcomeWrapperAlgorithm`. Both have
in-file comments explaining why; keep those comments when copying.

### 2.12 `xmtp.mls.message_contents` — wrapper_encryption.proto

```proto
// Encryption algorithms for the Welcome Wrapper
syntax = "proto3";

package xmtp.mls.message_contents;

option go_package = "github.com/xmtp/proto/v3/go/mls/message_contents";
option java_package = "org.xmtp.proto.mls.message.contents";

// Describes the algorithm used to encrypt the Welcome Wrapper
enum WelcomeWrapperAlgorithm {
  WELCOME_WRAPPER_ALGORITHM_UNSPECIFIED = 0;
  WELCOME_WRAPPER_ALGORITHM_CURVE25519 = 1;
  WELCOME_WRAPPER_ALGORITHM_XWING_MLKEM_768_DRAFT_6 = 2;
  // Only used for WelcomePointee's
  WELCOME_WRAPPER_ALGORITHM_SYMMETRIC_KEY = 3;
}

// The KeyPackageExtension that stores the PubKey and the WelcomeWrapperEncryption
message WelcomeWrapperEncryption {
  bytes pub_key = 1;
  WelcomeWrapperAlgorithm algorithm = 2;
}
```

### 2.13 `xmtp.identity.associations` — association.proto

`identity/associations/association.proto` in full:

```proto
// Payloads to be signed for identity associations
syntax = "proto3";

package xmtp.identity.associations;

import "identity/associations/signature.proto";

option go_package = "github.com/xmtp/proto/v3/go/identity/associations";
option java_package = "org.xmtp.proto.identity.associations";

// The identifier for a member of an XID
message MemberIdentifier {
  oneof kind {
    string ethereum_address = 1;
    bytes installation_public_key = 2;
    Passkey passkey = 3;
  }
}

// Passkey identifier
message Passkey {
  bytes key = 1;
  optional string relying_party = 2;
}

// List of identity kinds
enum IdentifierKind {
  IDENTIFIER_KIND_UNSPECIFIED = 0; // Ethereum on old clients
  IDENTIFIER_KIND_ETHEREUM = 1;
  IDENTIFIER_KIND_PASSKEY = 2;
}

// single member that optionally indicates the member that added them
message Member {
  MemberIdentifier identifier = 1;
  optional MemberIdentifier added_by_entity = 2;
  optional uint64 client_timestamp_ns = 3;
  optional uint64 added_on_chain_id = 4;
}

// The first entry of any XID log. The XID must be deterministically derivable
// from the address and nonce.
// The recovery address defaults to the initial associated_address unless
// there is a subsequent ChangeRecoveryAddress in the log.
message CreateInbox {
  string initial_identifier = 1;
  uint64 nonce = 2;
  Signature initial_identifier_signature = 3; // Must be an addressable member
  IdentifierKind initial_identifier_kind = 4;
  // Should be provided if identifier kind is passkey
  optional string relying_party = 5;
}

// Adds a new member for an XID - either an addressable member such as a
// wallet, or an installation acting on behalf of an address.
// A key-pair that has been associated with one role MUST not be permitted to be
// associated with a different role.
message AddAssociation {
  MemberIdentifier new_member_identifier = 1;
  Signature existing_member_signature = 2;
  Signature new_member_signature = 3;
  // Should be provided if identifier kind is passkey
  optional string relying_party = 4;
}

// Revokes a member from an XID. The recovery address must sign the revocation.
message RevokeAssociation {
  MemberIdentifier member_to_revoke = 1;
  Signature recovery_identifier_signature = 2;
}

// Changes the recovery identifier for an XID. The recovery identifier is not required
// to be a member of the XID. In addition to being able to add members, the
// recovery identifier can also revoke members.
message ChangeRecoveryAddress {
  string new_recovery_identifier = 1;
  Signature existing_recovery_identifier_signature = 2;
  IdentifierKind new_recovery_identifier_kind = 3;
  // Should be provided if identifier kind is passkey
  optional string relying_party = 4;
}

// A single identity operation
message IdentityAction {
  oneof kind {
    CreateInbox create_inbox = 1;
    AddAssociation add = 2;
    RevokeAssociation revoke = 3;
    ChangeRecoveryAddress change_recovery_address = 4;
  }
}

// One or more identity actions that were signed together.
// Example: [CreateXid, AddAssociation, ChangeRecoveryAddress]
// 1. The batched signature text is created by concatenating the signature text
//    of each association together with a separator, '\n\n\n'.
// 2. The user signs this concatenated result.
// 3. The resulting signature is added to each association proto where relevant.
//    The same signature may be used for multiple associations in the array.
message IdentityUpdate {
  repeated IdentityAction actions = 1;
  uint64 client_timestamp_ns = 2;
  string inbox_id = 3;
}

// Map of members belonging to an inbox_id
message MemberMap {
  MemberIdentifier key = 1;
  Member value = 2;
}

// A final association state resulting from multiple `IdentityUpdates`
message AssociationState {
  string inbox_id = 1;
  repeated MemberMap members = 2;
  string recovery_identifier = 3;
  repeated bytes seen_signatures = 4;
  IdentifierKind recovery_identifier_kind = 5;
  // Should be provided if identifier kind is passkey
  optional string relying_party = 6;
}

/// state diff between two final AssociationStates
message AssociationStateDiff {
  repeated MemberIdentifier new_members = 1;
  repeated MemberIdentifier removed_members = 2;
}
```

`IdentityUpdate.inbox_id` is a **hex string**, not bytes. The topic derivation
hex-decodes it: see §3.2.

`AssociationState` and `AssociationStateDiff` are server-side only in the sense
that only `mls_validation` returns them; the self-hosted backend must compute
association state itself if it validates identity updates (see §5-Q9).

### 2.14 `xmtp.identity.associations` — signature.proto

`identity/associations/signature.proto` in full:

```proto
// Signing methods for identity associations
syntax = "proto3";

package xmtp.identity.associations;

import "message_contents/public_key.proto";

option go_package = "github.com/xmtp/proto/v3/go/identity/associations";
option java_package = "org.xmtp.proto.identity.associations";

// RecoverableEcdsaSignature for EIP-191 and V2 signatures
message RecoverableEcdsaSignature {
  // 65-bytes [ R || S || V ], with recovery id as the last byte
  bytes bytes = 1;
}

// EdDSA signature for 25519
message RecoverableEd25519Signature {
  // 64 bytes [R(32 bytes) || S(32 bytes)]
  bytes bytes = 1;
  // 32 bytes
  bytes public_key = 2;
}

// Smart Contract Wallet signature
message SmartContractWalletSignature {
  // CAIP-10 string
  // https://github.com/ChainAgnostic/CAIPs/blob/main/CAIPs/caip-10.md
  string account_id = 1;
  // Specify the block number to verify the signature against
  uint64 block_number = 2;
  // The actual signature bytes
  bytes signature = 3;
}

// Passkey signature
message RecoverablePasskeySignature {
  bytes public_key = 1;
  bytes signature = 2;
  bytes authenticator_data = 3;
  bytes client_data_json = 4;
}

// An existing address on xmtpv2 may have already signed a legacy identity key
// of type SignedPublicKey via the 'Create Identity' signature.
// For migration to xmtpv3, the legacy key is permitted to sign on behalf of the
// address to create a matching xmtpv3 installation key.
// This signature type can ONLY be used for CreateXid and AddAssociation
// payloads, and can only be used once in xmtpv3.
message LegacyDelegatedSignature {
  xmtp.message_contents.SignedPublicKey delegated_key = 1;
  RecoverableEcdsaSignature signature = 2;
}

// A wrapper for all possible signature types
message Signature {
  // Must have two properties:
  // 1. An identifier (address or public key) for the signer must either be
  //    recoverable, or specified as a field.
  // 2. The signer certifies that the signing payload is correct. The payload
  //    must be inferred from the context in which the signature is provided.
  oneof signature {
    RecoverableEcdsaSignature erc_191 = 1;
    SmartContractWalletSignature erc_6492 = 2;
    RecoverableEd25519Signature installation_key = 3;
    LegacyDelegatedSignature delegated_erc_191 = 4;
    RecoverablePasskeySignature passkey = 5;
  }
}
```

`LegacyDelegatedSignature` is the sole reason the identity package depends on
the v2 `message_contents` package. If the self-hosted backend drops v2-migration
support, `message_contents/public_key.proto` and `message_contents/signature.proto`
fall out of the closure entirely.

### 2.15 `xmtp.message_contents` — public_key.proto

`message_contents/public_key.proto` in full (in the closure only via
`LegacyDelegatedSignature`):

```proto
// Structure for representing public keys of different types,
// including signatures used to authenticate the keys.
syntax = "proto3";

package xmtp.message_contents;

import "message_contents/signature.proto";

option go_package = "github.com/xmtp/proto/v3/go/message_contents";
option java_package = "org.xmtp.proto.message.contents";

// UnsignedPublicKey represents a generalized public key,
// defined as a union to support cryptographic algorithm agility.
message UnsignedPublicKey {
  uint64 created_ns = 1;
  oneof union {
    Secp256k1Uncompressed secp256k1_uncompressed = 3;
  }

  // Supported key types

  // EC: SECP256k1
  message Secp256k1Uncompressed {
    // uncompressed point with prefix (0x04) [ P || X || Y ], 65 bytes
    bytes bytes = 1;
  }
}

// SignedPublicKey
message SignedPublicKey {
  bytes key_bytes = 1; // embeds an UnsignedPublicKey
  Signature signature = 2; // signs key_bytes
}

// PublicKeyBundle packages the cryptographic keys associated with a wallet.
message SignedPublicKeyBundle {
  // Identity key MUST be signed by the wallet.
  SignedPublicKey identity_key = 1;
  // Pre-key MUST be signed by the identity key.
  SignedPublicKey pre_key = 2;
}

// LEGACY

// PublicKey represents a generalized public key,
// defined as a union to support cryptographic algorithm agility.
message PublicKey {
  // The key bytes
  message Secp256k1Uncompressed {
    // uncompressed point with prefix (0x04) [ P || X || Y ], 65 bytes
    bytes bytes = 1;
  }
  uint64 timestamp = 1;
  optional Signature signature = 2;
  oneof union {
    Secp256k1Uncompressed secp256k1_uncompressed = 3;
  }
}

// PublicKeyBundle packages the cryptographic keys associated with a wallet,
// both senders and recipients are identified by their key bundles.
message PublicKeyBundle {
  // Identity key MUST be signed by the wallet.
  PublicKey identity_key = 1;
  // Pre-key MUST be signed by the identity key.
  PublicKey pre_key = 2;
}
```

`SignedPublicKey.key_bytes` is a nested serialization: raw protobuf bytes of an
`UnsignedPublicKey`, so that the signature covers exact bytes.

### 2.16 `xmtp.message_contents` — signature.proto

`message_contents/signature.proto` in full:

```proto
// Signature is a generic structure for public key signatures.
syntax = "proto3";

package xmtp.message_contents;

option go_package = "github.com/xmtp/proto/v3/go/message_contents";
option java_package = "org.xmtp.proto.message.contents";

// Signature represents a generalized public key signature,
// defined as a union to support cryptographic algorithm agility.
message Signature {
  // ECDSA signature bytes and the recovery bit
  message ECDSACompact {
    bytes bytes = 1; // compact representation [ R || S ], 64 bytes
    uint32 recovery = 2; // recovery bit
  }
  // ECDSA signature bytes and the recovery bit
  // produced by xmtp-js::PublicKey.signWithWallet function, i.e.
  // EIP-191 signature of a "Create Identity" message with the key embedded.
  // Used to sign identity keys.
  message WalletECDSACompact {
    bytes bytes = 1; // compact representation [ R || S ], 64 bytes
    uint32 recovery = 2; // recovery bit
  }
  oneof union {
    ECDSACompact ecdsa_compact = 1;
    WalletECDSACompact wallet_ecdsa_compact = 2;
  }
}
```

Two `Signature` messages with the same simple name exist in the repo:
`xmtp.message_contents.Signature` (this one, 64-byte compact + recovery uint32)
and `xmtp.identity.associations.Signature` (the union, §2.14). They are not
interchangeable. See §5-Q5.

### 2.17 `xmtp.identity.api.v1` — the whole file

`identity/api/v1/identity.proto` in full:

```proto
// Message API
syntax = "proto3";
package xmtp.identity.api.v1;

import "google/api/annotations.proto";
import "identity/associations/association.proto";
import "protoc-gen-openapiv2/options/annotations.proto";

option go_package = "github.com/xmtp/proto/v3/go/mls/api/v1";
option java_package = "org.xmtp.proto.mls.api.v1";
option (grpc.gateway.protoc_gen_openapiv2.options.openapiv2_swagger) = {
  info: {
    title: "IdentityApi"
    version: "1.0"
  }
};

// RPCs for the new MLS API
service IdentityApi {
  // Publishes an identity update for an XID or wallet. An identity update may
  // consist of multiple identity actions that have been batch signed.
  rpc PublishIdentityUpdate(PublishIdentityUpdateRequest) returns (PublishIdentityUpdateResponse) {
    option (google.api.http) = {
      post: "/identity/v1/publish-identity-update"
      body: "*"
    };
  }

  // Used to check for changes related to members of a group.
  // Would return an array of any new installations associated with the wallet
  // address, and any revocations that have happened.
  rpc GetIdentityUpdates(GetIdentityUpdatesRequest) returns (GetIdentityUpdatesResponse) {
    option (google.api.http) = {
      post: "/identity/v1/get-identity-updates"
      body: "*"
    };
  }

  // Retrieve the XIDs for the given addresses
  rpc GetInboxIds(GetInboxIdsRequest) returns (GetInboxIdsResponse) {
    option (google.api.http) = {
      post: "/identity/v1/get-inbox-ids"
      body: "*"
    };
  }

  // Verify an unverified smart contract wallet signature
  rpc VerifySmartContractWalletSignatures(VerifySmartContractWalletSignaturesRequest) returns (VerifySmartContractWalletSignaturesResponse) {
    option (google.api.http) = {
      post: "/identity/v1/verify-smart-contract-wallet-signatures"
      body: "*"
    };
  }
}

message VerifySmartContractWalletSignaturesRequest {
  repeated VerifySmartContractWalletSignatureRequestSignature signatures = 1;
}

message VerifySmartContractWalletSignatureRequestSignature {
  // CAIP-10 string
  // https://github.com/ChainAgnostic/CAIPs/blob/main/CAIPs/caip-10.md
  string account_id = 1;
  // Specify the block number to verify the signature against
  optional uint64 block_number = 2;
  // The signature bytes
  bytes signature = 3;
  bytes hash = 4;
}

message VerifySmartContractWalletSignaturesResponse {
  message ValidationResponse {
    bool is_valid = 1;
    optional uint64 block_number = 2;
    optional string error = 3;
  }

  repeated ValidationResponse responses = 1;
}

// Publishes an identity update to the network
message PublishIdentityUpdateRequest {
  xmtp.identity.associations.IdentityUpdate identity_update = 1;
}

// The response when an identity update is published
message PublishIdentityUpdateResponse {}

// Get all updates for an identity since the specified time
message GetIdentityUpdatesRequest {
  // Points to the last entry the client has received. The sequence_id should be
  // set to 0 if the client has not received anything.
  message Request {
    string inbox_id = 1;
    uint64 sequence_id = 2;
  }

  repeated Request requests = 1;
}

// Returns all log entries for the requested identities
message GetIdentityUpdatesResponse {
  // A single entry in the XID log on the server.
  message IdentityUpdateLog {
    uint64 sequence_id = 1;
    uint64 server_timestamp_ns = 2;
    xmtp.identity.associations.IdentityUpdate update = 3;
  }

  // The update log for a single identity, starting after the last cursor
  message Response {
    string inbox_id = 1;
    repeated IdentityUpdateLog updates = 2;
  }

  repeated Response responses = 1;
}

// Request to retrieve the XIDs for the given addresses
message GetInboxIdsRequest {
  // A single request for a given address
  message Request {
    string identifier = 1;
    xmtp.identity.associations.IdentifierKind identifier_kind = 2;
  }

  repeated Request requests = 1;
}

// Response with the XIDs for the requested addresses
message GetInboxIdsResponse {
  // A single response for a given address
  message Response {
    string identifier = 1;
    optional string inbox_id = 2;
    xmtp.identity.associations.IdentifierKind identifier_kind = 3;
  }

  repeated Response responses = 1;
}
```

This file's `go_package` and `java_package` are **wrong**: both say
`mls/api/v1` / `org.xmtp.proto.mls.api.v1`, copy-pasted from `mls.proto`. Not
a problem for Rust (prost ignores them), but worth fixing or dropping when the
files move in-repo.

`GetInboxIdsResponse.Response.inbox_id` is `optional string` — absent means the
identifier has no inbox. That optionality must survive the port; a bare `string`
would make "no inbox" and "empty inbox id" indistinguishable.

### 2.18 `xmtp.identity` — credential.proto

`identity/credential.proto` in full:

```proto
// Credentials
syntax = "proto3";

package xmtp.identity;

option go_package = "github.com/xmtp/proto/v3/go/identity";
option java_package = "org.xmtp.proto.identity";

// A credential that can be used in MLS leaf nodes
message MlsCredential {
  string inbox_id = 1;
}
```

Not directly referenced by backend.proto, but it is the payload inside every
key package's MLS credential. If the backend validates uploaded key packages
(as the current Go node does via `mls_validation`), it needs this file.

### 2.19 `xmtp.xmtpv4.envelopes` — envelopes.proto

`xmtpv4/envelopes/envelopes.proto` in full:

```proto
// Message API for XMTP V4
syntax = "proto3";

package xmtp.xmtpv4.envelopes;

import "identity/associations/association.proto";
import "identity/associations/signature.proto";
import "mls/api/v1/mls.proto";
import "xmtpv4/envelopes/payer_report.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/envelopes";

// The last seen entry per originator. Originators that have not been seen are omitted.
message Cursor {
  map<uint32, uint64> node_id_to_sequence_id = 1;
}

// Data visible to the server that has been authenticated by the client.
message AuthenticatedData {
  // Do NOT reuse tag 1 — previously used by target_originator
  bytes target_topic = 2;
  Cursor depends_on = 3;
  // Do NOT reuse tag 4 — previously used by is_commit
}

message ClientEnvelope {
  AuthenticatedData aad = 1;

  oneof payload {
    xmtp.mls.api.v1.GroupMessageInput group_message = 2;
    xmtp.mls.api.v1.WelcomeMessageInput welcome_message = 3;
    xmtp.mls.api.v1.UploadKeyPackageRequest upload_key_package = 4;
    xmtp.identity.associations.IdentityUpdate identity_update = 5;
    PayerReport payer_report = 6;
    PayerReportAttestation payer_report_attestation = 7;
  }
}

// Wraps client envelope with payer signature
message PayerEnvelope {
  bytes unsigned_client_envelope = 1; // Protobuf serialized
  xmtp.identity.associations.RecoverableEcdsaSignature payer_signature = 2;
  uint32 target_originator = 3;
  uint32 message_retention_days = 4;
}

// For blockchain envelopes, these fields are set by the smart contract
message UnsignedOriginatorEnvelope {
  uint32 originator_node_id = 1;
  uint64 originator_sequence_id = 2;
  int64 originator_ns = 3;
  bytes payer_envelope_bytes = 4;
  uint64 base_fee_picodollars = 5; // The base fee for the message in picodollars
  uint64 congestion_fee_picodollars = 6; // The congestion fee for the message in picodollars
  uint64 expiry_unixtime = 7;
}

// An alternative to a signature for blockchain payloads
message BlockchainProof {
  bytes transaction_hash = 1;
}

// Signed originator envelope
message OriginatorEnvelope {
  bytes unsigned_originator_envelope = 1; // Protobuf serialized
  oneof proof {
    xmtp.identity.associations.RecoverableEcdsaSignature originator_signature = 2;
    BlockchainProof blockchain_proof = 3;
  }
}
```

**Key differences from backend.proto's `ClientEnvelope`:**

| v4 `xmtp.xmtpv4.envelopes.ClientEnvelope` | `xmtp.backend.v1.ClientEnvelope` |
| --- | --- |
| has `AuthenticatedData aad = 1` | **no aad** — topic and depends-on are gone |
| `upload_key_package = 4` | renamed `key_package = 4` (same tag) |
| no commit-log payload | adds `commit_log_entry = 6` |
| `payer_report = 6`, `payer_report_attestation = 7` | dropped (no payers in self-hosted) |
| payload tags start at 2 (1 taken by `aad`) | **payload tags still start at 2** even though nothing occupies 1 — see §5-Q2 |

Note the two `Do NOT reuse tag` comments: `AuthenticatedData` once had
`target_originator` (tag 1) and `is_commit` (tag 4). **`is_commit` is no longer
part of `AuthenticatedData`** in this revision of the proto — a claim to check
against the design brief, which lists `is_commit` as something AuthenticatedData
carries. It does not, at commit `dedb872`. Today it lives on
`GroupMessage.V1.is_commit` (server-derived) and on
`xmtp.mls_validation.v1.ValidateGroupMessagesResponse.ValidationResponse.is_commit`.
The three-line `AuthenticatedData` therefore carries exactly:
`target_topic` (bytes) and `depends_on` (a vector `Cursor`).

The three-tier envelope nesting is:
`OriginatorEnvelope` → `unsigned_originator_envelope` bytes → `UnsignedOriginatorEnvelope`
→ `payer_envelope_bytes` → `PayerEnvelope` → `unsigned_client_envelope` bytes →
`ClientEnvelope` → `payload` oneof. Each level re-serializes protobuf into a
`bytes` field so signatures cover exact wire bytes. backend.proto collapses this
to `ServerEnvelope { EnvelopeMeta meta; ClientEnvelope envelope }` with no
nested serialization and no signatures.

### 2.20 `xmtp.xmtpv4.envelopes` — payer_report.proto

```proto
// Message API for XMTP V4
syntax = "proto3";

package xmtp.xmtpv4.envelopes;

import "identity/associations/signature.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/envelopes";

// A report of the payers and nodes that sent messages in a given range of messages
message PayerReport {
  // The originator this report is referring to
  uint32 originator_node_id = 1;
  // The sequence_id that the report starts at [exclusive]
  uint64 start_sequence_id = 2;
  // The sequence_id that the report ends at [inclusive]
  uint64 end_sequence_id = 3;
  // The end timestamp of the report
  uint32 end_minute_since_epoch = 4;
  // The merkle root of the payer balance diff tree
  bytes payers_merkle_root = 5;
  // The node IDs that are active in the network at the time of the report
  repeated uint32 active_node_ids = 6;
}

message NodeSignature {
  uint32 node_id = 1;
  xmtp.identity.associations.RecoverableEcdsaSignature signature = 2;
}

// An attestation of a payer report
message PayerReportAttestation {
  // The ID of the report, determined by hashing the report contents
  bytes report_id = 1;
  // The signature of the attester
  NodeSignature signature = 2;
}
```

Pure economics/settlement. Drop entirely for the self-hosted backend.

### 2.21 `xmtp.xmtpv4.message_api` — message_api.proto

`xmtpv4/message_api/message_api.proto` in full (336 lines). Header and query
types:

```proto
// Message API for XMTP V4
syntax = "proto3";

package xmtp.xmtpv4.message_api;

import "identity/associations/association.proto";
import "xmtpv4/envelopes/envelopes.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/message_api";

// Query for envelopes, shared by query and subscribe endpoints
// Either topics or originator_node_ids may be set, but not both
message EnvelopesQuery {
  // Client queries
  repeated bytes topics = 1;
  // Node queries
  repeated uint32 originator_node_ids = 2;
  xmtp.xmtpv4.envelopes.Cursor last_seen = 3;
}

// Batch subscribe to envelopes
message SubscribeEnvelopesRequest {
  EnvelopesQuery query = 1;
}

// Request to subscribe to a series of topics, with a separate cursor for each topic
message SubscribeTopicsRequest {
  message TopicFilter {
    bytes topic = 1;
    xmtp.xmtpv4.envelopes.Cursor last_seen = 2;
  }

  repeated TopicFilter filters = 1;
}

// Response to SubscribeTopics
message SubscribeTopicsResponse {
  enum SubscriptionStatus {
    SUBSCRIPTION_STATUS_UNSPECIFIED = 0;
    SUBSCRIPTION_STATUS_STARTED = 1;
    SUBSCRIPTION_STATUS_CATCHUP_COMPLETE = 2;
    SUBSCRIPTION_STATUS_WAITING = 3;
  }

  message StatusUpdate {
    SubscriptionStatus status = 1;
  }

  message Envelopes {
    repeated xmtp.xmtpv4.envelopes.OriginatorEnvelope envelopes = 1;
  }

  oneof response {
    Envelopes envelopes = 1;
    StatusUpdate status_update = 2;
  }
}

// Streamed response for batch subscribe - can be multiple envelopes at once
message SubscribeEnvelopesResponse {
  repeated xmtp.xmtpv4.envelopes.OriginatorEnvelope envelopes = 1;
}
```

`EnvelopesQuery` has the "either topics or originator_node_ids, but not both"
constraint expressed only in a comment, not in the type. backend.proto's
`QueryRequest { repeated TopicQuery queries; uint32 limit }` removes the
ambiguity by dropping node queries entirely.

XIP-83 Subscribe block, `message_api.proto:64-214` (identical structure to the
v3 one in §2.6, differing only in the cursor type and the payload type):

```proto
// ---- XIP-83 bidirectional mutable subscription (d14n binding) ----
//
// QueryApi.Subscribe is the bidirectional evolution of SubscribeTopics: one
// long-lived stream the client mutates in place (no reconnect on group
// join/leave) with a WebSocket-style ping/pong so silent stream death is
// detected on both ends. It mirrors the v3 MlsApi.Subscribe control protocol
// (mls/api/v1/mls.proto), adapted to the decentralized data model: each
// subscription resumes from a per-originator vector Cursor, and delivery is the
// unified OriginatorEnvelope stream rather than typed group/welcome messages
// (the client demuxes by each envelope's target topic). SubscribeTopics remains
// the server-streaming, immutable ancestor for clients that cannot do bidi
// (grpc-web / connect-web). Request and response are wrapped in `oneof version`
// and pinned per stream: V1 requests receive only V1 responses.

// Client -> node. Sent one or more times over the life of the stream.
message SubscribeRequest {
  oneof version {
    V1 v1 = 1;
  }

  message V1 {
    // Each frame is exactly one of: a mutation, a Ping, or a Pong.
    oneof request {
      Mutate mutate = 1;
      Ping ping = 2; // liveness challenge (e.g. probe the link after resuming)
      Pong pong = 3; // answer to a node Ping
    }

    // Add and/or remove subscriptions in place (applied atomically per frame).
    // Topics use the kind-prefixed binary representation (XIP-49 §3.3.2): the
    // first byte is the topic kind, the remainder is the identifier. A topic
    // whose kind the node does not serve fails the stream with INVALID_ARGUMENT.
    message Mutate {
      repeated Subscription adds = 1; // begin delivering these topics
      repeated bytes removes = 2; // stop delivering; clears the topic's cursor floor so a re-add replays

      // Catch this Mutate's adds up — history, TopicsLive markers, and the
      // wave's CatchupComplete — but do NOT register them for live delivery.
      // The markers then mean "you have everything as of the wave's start";
      // later envelopes arrive on no lane of this stream. Combined with
      // half-closing the request stream, this is the bounded catch-up ("sync")
      // mode: the node finishes the wave then closes the stream itself.
      // Removals in the Mutate are unaffected.
      bool history_only = 3;

      // Client-chosen correlation id: echoed on this wave's CatchupComplete,
      // and stamped on every delivery frame of the wave's catch-up replay
      // (Envelopes.mutate_id). MUST be nonzero when adds are present (0 is the
      // live tag), and MUST NOT match the mutate_id of a wave still in flight
      // on the stream (an in-flight collision would make two waves' frames and
      // completions indistinguishable) — either violation fails the stream
      // with INVALID_ARGUMENT. SHOULD be unique per stream so completed waves
      // stay attributable too.
      uint64 mutate_id = 4;

      // A topic to subscribe, with the vector cursor to resume from.
      message Subscription {
        bytes topic = 1;
        // Resume point: deliver envelopes beyond this per-originator vector
        // cursor. Omitted/empty = from the beginning. Originators absent from
        // the cursor map are treated as sequence 0 (the node fills them in),
        // mirroring SubscribeTopics.TopicFilter.last_seen.
        xmtp.xmtpv4.envelopes.Cursor last_seen = 2;
      }
    }
  }
}

// Node -> client.
message SubscribeResponse {
  oneof version {
    V1 v1 = 1;
  }

  message V1 {
    oneof response {
      Envelopes envelopes = 1;
      Started started = 2; // sent once, immediately on open, before any catch-up
      Ping ping = 3; // idle liveness challenge; receiver MUST answer with Pong
      Pong pong = 4; // answer to a client Ping
      TopicsLive topics_live = 5; // no more replay for these topics; live begins after CatchupComplete
      CatchupComplete catchup_complete = 6; // acks a Mutate; wave completion if it started one
    }

    // A batch of envelopes across the active subscriptions; the client demuxes
    // by each envelope's target topic. A frame belongs to exactly one catch-up
    // wave or to live — the node never mixes lanes, or two waves, in one frame
    // — and each lane delivers every originator's envelopes in ascending
    // originator_sequence_id (live: across all live topics on the stream; a
    // wave: across the wave's topics).
    message Envelopes {
      repeated xmtp.xmtpv4.envelopes.OriginatorEnvelope envelopes = 1;
      // The catch-up wave that produced this frame: the Mutate's mutate_id
      // for wave replay, 0 for live tail.
      uint64 mutate_id = 2;
    }

    // The first frame on every stream.
    message Started {
      // The node's ping cadence (ms): the basis for the client's staleness
      // threshold and the node's reap deadline.
      uint32 keepalive_interval_ms = 1;
      // Optional protocol features the node supports on this stream. The node
      // silently ignores request types it does not understand, so a client MUST
      // NOT send an optional request type whose capability the node did not
      // advertise (it would hang waiting on a response that never comes).
      repeated Capability capabilities = 2;
    }

    // Sent once per Mutate: at wave completion (after the wave's last
    // TopicsLive) for a Mutate that started a catch-up "wave", immediately for
    // one that did not (nothing added — removes-only or empty — or every add
    // a no-op). Also the catch-up
    // seam: live frames (mutate_id 0) for the wave's topics begin only after
    // this frame.
    message CatchupComplete {
      uint64 mutate_id = 1; // echoes the Mutate; 0 only if a waveless Mutate carried 0
    }

    // Emitted when topics finish catch-up, AFTER the last history frame for
    // them — including envelopes that arrived mid-wave and were folded into it,
    // which were equally historical from the client's perspective — so no
    // further replay for a listed topic follows; its live (mutate_id 0) frames
    // begin after the wave's CatchupComplete. Informational only: delivery
    // correctness (no duplicates, no gaps) never depends on it. Re-adding a
    // topic re-runs catch-up and re-emits it; receivers treat it idempotently.
    message TopicsLive {
      repeated bytes topics = 1; // kind-prefixed topics done replaying
    }

    // Optional per-stream protocol features (none defined yet; future revisions
    // add values, e.g. fetch-over-stream lookups answered with the same read
    // view that feeds the stream, or new streamable topic kinds).
    enum Capability {
      CAPABILITY_UNSPECIFIED = 0;
    }
  }
}

// Liveness challenge/response for Subscribe, shared across versions. Either peer
// MAY send a Ping; the receiver MUST reply with a Pong echoing the nonce. The
// sender closes the stream if no Pong arrives within its deadline — how a node
// reaps a vanished peer (e.g. a mobile client the OS suspended behind a proxy
// that still ACKs the transport).
message Ping {
  uint64 nonce = 1;
}

message Pong {
  uint64 nonce = 1; // echoes the nonce of the Ping it answers
}
```

Query / publish / inbox-id / newest-envelope messages, `message_api.proto:216-294`:

```proto
// Batch subscribe to all envelopes
message SubscribeAllEnvelopesRequest {}

// Query envelopes request
message QueryEnvelopesRequest {
  EnvelopesQuery query = 1;
  uint32 limit = 2;
}

// Query envelopes response
message QueryEnvelopesResponse {
  repeated xmtp.xmtpv4.envelopes.OriginatorEnvelope envelopes = 1;
}

message PublishPayerEnvelopesRequest {
  repeated xmtp.xmtpv4.envelopes.PayerEnvelope payer_envelopes = 1;
}

message PublishPayerEnvelopesResponse {
  repeated xmtp.xmtpv4.envelopes.OriginatorEnvelope originator_envelopes = 1;
}

// Request to retrieve the XIDs for the given addresses
message GetInboxIdsRequest {
  // A single request for a given address
  message Request {
    string identifier = 1;
    xmtp.identity.associations.IdentifierKind identifier_kind = 2;
  }

  repeated Request requests = 1;
}

// Response with the XIDs for the requested addresses
message GetInboxIdsResponse {
  // A single response for a given address
  message Response {
    string identifier = 1;
    optional string inbox_id = 2;
    xmtp.identity.associations.IdentifierKind identifier_kind = 3;
  }

  repeated Response responses = 1;
}

// Request to get the newest envelope for a given topic
message GetNewestEnvelopeRequest {
  repeated bytes topics = 1;
}

// Response to GetNewestEnvelopeRequest
message GetNewestEnvelopeResponse {
  message Response {
    optional xmtp.xmtpv4.envelopes.OriginatorEnvelope originator_envelope = 1;
  }
  // The newest envelope for the given topic OR null if there are no envelopes on the topic
  repeated Response results = 1;
}

// Subscribe to envelopes from specific originator nodes
message SubscribeOriginatorsRequest {
  message OriginatorFilter {
    repeated uint32 originator_node_ids = 1;
    xmtp.xmtpv4.envelopes.Cursor last_seen = 2;
  }

  OriginatorFilter filter = 1;
}

// Response for SubscribeOriginators
message SubscribeOriginatorsResponse {
  message Envelopes {
    repeated xmtp.xmtpv4.envelopes.OriginatorEnvelope envelopes = 1;
  }

  oneof response {
    Envelopes envelopes = 1;
  }
}
```

`xmtp.xmtpv4.message_api.GetInboxIdsRequest/Response` are byte-identical in
field shape to `xmtp.identity.api.v1.GetInboxIdsRequest/Response` (§2.17): same
field names, same numbers, same `optional string inbox_id`. They are duplicated
across packages only so the v4 API does not depend on `identity/api/v1`. Both
still depend on `identity/associations/association.proto` for `IdentifierKind`.
See §5-Q1.

`ReplicationApi` service, `message_api.proto:296-336`:

```proto
service ReplicationApi {
  // Node-to-node originator subscription
  rpc SubscribeOriginators(SubscribeOriginatorsRequest)
      returns (stream SubscribeOriginatorsResponse) {}

  // Deprecated: use SubscribeOriginators for node queries,
  // QueryApi.SubscribeTopics for client queries
  rpc SubscribeEnvelopes(SubscribeEnvelopesRequest)
      returns (stream SubscribeEnvelopesResponse) {
    option deprecated = true;
  }

  // Deprecated: moved to QueryApi
  rpc SubscribeTopics(SubscribeTopicsRequest)
      returns (stream SubscribeTopicsResponse) {
    option deprecated = true;
  }

  // Deprecated: moved to QueryApi
  rpc QueryEnvelopes(QueryEnvelopesRequest)
      returns (QueryEnvelopesResponse) {
    option deprecated = true;
  }

  // Deprecated: moved to PublishApi
  rpc PublishPayerEnvelopes(PublishPayerEnvelopesRequest)
      returns (PublishPayerEnvelopesResponse) {
    option deprecated = true;
  }

  // Deprecated: moved to QueryApi
  rpc GetInboxIds(GetInboxIdsRequest) returns (GetInboxIdsResponse) {
    option deprecated = true;
  }

  // Deprecated: moved to QueryApi
  rpc GetNewestEnvelope(GetNewestEnvelopeRequest)
      returns (GetNewestEnvelopeResponse) {
    option deprecated = true;
  }
}
```

### 2.22 `xmtp.xmtpv4.message_api` — service split files

Three small files split the services out of the message-file, all reusing the
same message types in the same package.

`xmtpv4/message_api/query_api.proto` in full:

```proto
// Query API - Client to Node queries and subscriptions
syntax = "proto3";

package xmtp.xmtpv4.message_api;

import "xmtpv4/message_api/message_api.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/message_api";

// Client -> Node. No auth token required.
service QueryApi {
  rpc QueryEnvelopes(QueryEnvelopesRequest) returns (QueryEnvelopesResponse) {}

  rpc SubscribeTopics(SubscribeTopicsRequest)
      returns (stream SubscribeTopicsResponse) {}

  // XIP-83 bidirectional mutable subscription: a single long-lived stream the
  // client mutates in place (add/remove topics) with ping/pong liveness, in
  // contrast to SubscribeTopics' fixed, immutable, server-streaming filter set.
  // Bidi streaming requires HTTP/2 (not grpc-web / connect-web); browser
  // clients stay on SubscribeTopics.
  rpc Subscribe(stream SubscribeRequest) returns (stream SubscribeResponse) {}

  rpc GetInboxIds(GetInboxIdsRequest) returns (GetInboxIdsResponse) {}

  rpc GetNewestEnvelope(GetNewestEnvelopeRequest)
      returns (GetNewestEnvelopeResponse) {}
}
```

This is the closest existing analogue of backend.proto's `QueryService` +
`SubscriptionService` + `IdentityService.GetInboxIds` combined.

`xmtpv4/message_api/publish_api.proto` in full:

```proto
// Publish API - Gateway to Node publishing
syntax = "proto3";

package xmtp.xmtpv4.message_api;

import "xmtpv4/message_api/message_api.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/message_api";

// Gateway -> Node.
service PublishApi {
  rpc PublishPayerEnvelopes(PublishPayerEnvelopesRequest)
      returns (PublishPayerEnvelopesResponse) {}
}
```

`xmtpv4/message_api/notification_api.proto` in full:

```proto
// Notification API - Full envelope stream for push notification servers
syntax = "proto3";

package xmtp.xmtpv4.message_api;

import "xmtpv4/message_api/message_api.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/message_api";

// Full envelope stream for notification services.
service NotificationApi {
  rpc SubscribeAllEnvelopes(SubscribeAllEnvelopesRequest)
      returns (stream SubscribeEnvelopesResponse) {}
}
```

`xmtpv4/message_api/misbehavior_api.proto` (76 lines) defines node misbehavior
reporting. No libxmtp call sites. Not reproduced here.

### 2.23 `xmtp.xmtpv4.payer_api` and `gateway_api`

`xmtpv4/payer_api/payer_api.proto` in full:

```proto
// Payer API
syntax = "proto3";

package xmtp.xmtpv4.payer_api;

import "xmtpv4/envelopes/envelopes.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/payer_api";

message PublishClientEnvelopesRequest {
  repeated xmtp.xmtpv4.envelopes.ClientEnvelope envelopes = 1;
}

message PublishClientEnvelopesResponse {
  repeated xmtp.xmtpv4.envelopes.OriginatorEnvelope originator_envelopes = 1;
}

message GetNodesRequest {}

message GetNodesResponse {
  map<uint32, string> nodes = 1;
}

// Deprecated: use gateway_api.GatewayApi
service PayerApi {
  option deprecated = true;

  rpc PublishClientEnvelopes(PublishClientEnvelopesRequest)
      returns (PublishClientEnvelopesResponse) {}

  rpc GetNodes(GetNodesRequest) returns (GetNodesResponse) {}
}
```

libxmtp still hard-codes the deprecated path:
`crates/xmtp_api_d14n/src/endpoints/d14n/publish_client_envelopes.rs:27` returns
`"/xmtp.xmtpv4.payer_api.PayerApi/PublishClientEnvelopes"`, and
`crates/xmtp_api_d14n/src/middleware/multi_node_client/gateway_api.rs:226`
expects `"/xmtp.xmtpv4.payer_api.PayerApi/GetNodes"`. The `GatewayApi` service
that replaces it is never used, and is not even compiled (§1.3).

`xmtpv4/gateway_api/gateway_api.proto` in full:

```proto
// Gateway API - Client to Gateway requests
syntax = "proto3";

package xmtp.xmtpv4.gateway_api;

import "xmtpv4/payer_api/payer_api.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/gateway_api";

// Client -> Gateway. Replaces payer_api.PayerApi.
service GatewayApi {
  rpc PublishClientEnvelopes(xmtp.xmtpv4.payer_api.PublishClientEnvelopesRequest)
      returns (xmtp.xmtpv4.payer_api.PublishClientEnvelopesResponse) {}

  rpc GetNodes(xmtp.xmtpv4.payer_api.GetNodesRequest)
      returns (xmtp.xmtpv4.payer_api.GetNodesResponse) {}
}
```

`PublishClientEnvelopesRequest` is the closest existing analogue of
backend.proto's `PublishRequest { repeated ClientEnvelope envelopes }`.
backend.proto's `PublishResponse { repeated EnvelopeMeta envelope_metas }`
replaces v4's `repeated OriginatorEnvelope` — returning metadata rather than
echoing the whole signed envelope back.

### 2.24 `xmtp.xmtpv4.metadata_api`

`xmtpv4/metadata_api/metadata_api.proto` in full. Compiled into libxmtp but
with zero call sites.

```proto
// Metadata API
syntax = "proto3";

package xmtp.xmtpv4.metadata_api;

import "xmtpv4/envelopes/envelopes.proto";

option go_package = "github.com/xmtp/proto/v3/go/xmtpv4/metadata_api";

message GetSyncCursorRequest {}

message GetSyncCursorResponse {
  xmtp.xmtpv4.envelopes.Cursor latest_sync = 1;
}

message GetVersionRequest {}

message GetVersionResponse {
  string version = 1;
}

// Whether to group spend by hour or day
enum PayerInfoGranularity {
  PAYER_INFO_GRANULARITY_UNSPECIFIED = 0;
  PAYER_INFO_GRANULARITY_HOUR = 1;
  PAYER_INFO_GRANULARITY_DAY = 2;
}

// Get information about payer spend and message counts for a given time period
message GetPayerInfoRequest {
  repeated string payer_addresses = 1;
  PayerInfoGranularity granularity = 2;
}

// Response to GetPayerInfoRequest
message GetPayerInfoResponse {
  message PeriodSummary {
    uint64 amount_spent_picodollars = 1;
    uint64 num_messages = 2;
    uint64 period_start_unix_seconds = 3;
  }

  message PayerInfo {
    repeated PeriodSummary period_summaries = 1;
  }

  // Map of payer address
  map<string, PayerInfo> payer_info = 1;
}

// Metadata for distributed tracing, debugging and synchronization
service MetadataApi {
  rpc GetSyncCursor(GetSyncCursorRequest) returns (GetSyncCursorResponse) {}

  rpc SubscribeSyncCursor(GetSyncCursorRequest) returns (stream GetSyncCursorResponse) {}

  rpc GetVersion(GetVersionRequest) returns (GetVersionResponse) {}

  rpc GetPayerInfo(GetPayerInfoRequest) returns (GetPayerInfoResponse) {}
}
```

`GetVersion` is the only piece here that has an obvious analogue in a
self-hosted backend (a health/version endpoint); backend.proto currently has
none.

### 2.25 `xmtp.message_api.v1` — v3/v2 MessageApi (legacy)

`message_api/v1/message_api.proto` in full. Classification (d). libxmtp uses
only `SortDirection` from it, and only in one place.

```proto
// Message API
syntax = "proto3";
package xmtp.message_api.v1;

import "google/api/annotations.proto";
import "protoc-gen-openapiv2/options/annotations.proto";

option go_package = "github.com/xmtp/proto/v3/go/message_api/v1";
option java_package = "org.xmtp.proto.message.api.v1";
option (grpc.gateway.protoc_gen_openapiv2.options.openapiv2_swagger) = {
  info: {
    title: "MessageApi"
    version: "1.0"
  }
};

// RPC
service MessageApi {
  // Publish messages to the network
  rpc Publish(PublishRequest) returns (PublishResponse) {
    option (google.api.http) = {
      post: "/message/v1/publish"
      body: "*"
    };
  }
  // Subscribe to a stream of new envelopes matching a predicate
  rpc Subscribe(SubscribeRequest) returns (stream Envelope) {
    option (google.api.http) = {
      post: "/message/v1/subscribe"
      body: "*"
    };
  }
  // Subscribe to a stream of new envelopes and your subscription using
  // bidirectional streaming
  // protolint:disable:next RPC_REQUEST_STANDARD_NAME
  rpc Subscribe2(stream SubscribeRequest) returns (stream Envelope) {}
  // Subscribe to a stream of all messages
  rpc SubscribeAll(SubscribeAllRequest) returns (stream Envelope) {
    option (google.api.http) = {
      post: "/message/v1/subscribe-all"
      body: "*"
    };
  }
  // Query the store for messages
  rpc Query(QueryRequest) returns (QueryResponse) {
    option (google.api.http) = {
      post: "/message/v1/query"
      body: "*"
    };
  }
  // BatchQuery containing a set of queries to be processed
  rpc BatchQuery(BatchQueryRequest) returns (BatchQueryResponse) {
    option (google.api.http) = {
      post: "/message/v1/batch-query"
      body: "*"
    };
  }
}

// Sort direction
enum SortDirection {
  SORT_DIRECTION_UNSPECIFIED = 0;
  SORT_DIRECTION_ASCENDING = 1;
  SORT_DIRECTION_DESCENDING = 2;
}

// This is based off of the go-waku Index type, but with the
// receiverTime and pubsubTopic removed for simplicity.
// Both removed fields are optional
message IndexCursor {
  bytes digest = 1;
  uint64 sender_time_ns = 2;
}

// Wrapper for potentially multiple types of cursor
message Cursor {
  // Making the cursor a one-of type, as I would like to change the way we
  // handle pagination to use a precomputed sort field.
  // This way we can handle both methods
  oneof cursor {
    IndexCursor index = 1;
  }
}

// This is based off of the go-waku PagingInfo struct, but with the direction
// changed to our SortDirection enum format
message PagingInfo {
  // Note: this is a uint32, while go-waku's pageSize is a uint64
  uint32 limit = 1;
  Cursor cursor = 2;
  SortDirection direction = 3;
}

// Envelope encapsulates a message while in transit.
message Envelope {
  // The topic the message belongs to,
  // If the message includes the topic as well
  // it MUST be the same as the topic in the envelope.
  string content_topic = 1;
  // Message creation timestamp
  // If the message includes the timestamp as well
  // it MUST be equivalent to the timestamp in the envelope.
  uint64 timestamp_ns = 2;
  bytes message = 3;
}

// Publish
message PublishRequest {
  repeated Envelope envelopes = 1;
}

// Empty message as a response for Publish
message PublishResponse {}

// Subscribe
message SubscribeRequest {
  repeated string content_topics = 1;
}

// SubscribeAll
message SubscribeAllRequest {}

// Query
message QueryRequest {
  repeated string content_topics = 1;
  uint64 start_time_ns = 2;
  uint64 end_time_ns = 3;
  PagingInfo paging_info = 4;
}

// The response, containing envelopes, for a query
message QueryResponse {
  repeated Envelope envelopes = 1;
  PagingInfo paging_info = 2;
}

// BatchQuery
message BatchQueryRequest {
  repeated QueryRequest requests = 1;
}

// Response containing a list of QueryResponse messages
message BatchQueryResponse {
  repeated QueryResponse responses = 1;
}
```

`message_api/v1/authn.proto` in full (classification (d), zero libxmtp hits):

```proto
// Client authentication protocol
syntax = "proto3";
package xmtp.message_api.v1;

import "message_contents/public_key.proto";
import "message_contents/signature.proto";

option go_package = "github.com/xmtp/proto/v3/go/message_api/v1";
option java_package = "org.xmtp.proto.message.api.v1";

// Token is used by clients to prove to the nodes
// that they are serving a specific wallet.
message Token {
  // identity key signed by a wallet
  xmtp.message_contents.PublicKey identity_key = 1;
  // encoded bytes of AuthData
  bytes auth_data_bytes = 2;
  // identity key signature of AuthData bytes
  xmtp.message_contents.Signature auth_data_signature = 3;
}

// AuthData carries token parameters that are authenticated
// by the identity key signature.
// It is embedded in the Token structure as bytes
// so that the bytes don't need to be reconstructed
// to verify the token signature.
message AuthData {
  // address of the wallet
  string wallet_addr = 1;
  // time when the token was generated/signed
  uint64 created_ns = 2;
}
```

### 2.26 `xmtp.mls_validation.v1`

`mls_validation/v1/service.proto` in full. Classification (e). Implemented
server-side by `/Users/nickmolnar/code/xmtp/libxmtp/apps/mls_validation_service`.

```proto
// Message API
syntax = "proto3";
package xmtp.mls_validation.v1;

import "identity/api/v1/identity.proto";
import "identity/associations/association.proto";
import "identity/credential.proto";

option go_package = "github.com/xmtp/proto/v3/go/mls_validation/v1";

// RPCs for the new MLS API
service ValidationApi {
  // Validates and parses a group message and returns relevant details
  rpc ValidateGroupMessages(ValidateGroupMessagesRequest)
      returns (ValidateGroupMessagesResponse) {}

  // Gets the final association state for a batch of identity updates
  rpc GetAssociationState(GetAssociationStateRequest)
      returns (GetAssociationStateResponse) {}

  // Validates InboxID key packages and returns credential information for them,
  // without checking whether an InboxId <> InstallationPublicKey pair is really
  // valid.
  rpc ValidateInboxIdKeyPackages(ValidateKeyPackagesRequest)
      returns (ValidateInboxIdKeyPackagesResponse) {}

  // Verifies smart contracts
  // This request is proxied from the node, so we'll reuse those messages.
  rpc VerifySmartContractWalletSignatures(
      xmtp.identity.api.v1.VerifySmartContractWalletSignaturesRequest)
      returns (
          xmtp.identity.api.v1.VerifySmartContractWalletSignaturesResponse) {}
}

// Contains a batch of serialized Key Packages
message ValidateInboxIdKeyPackagesRequest {
  // Wrapper for each key package
  message KeyPackage {
    bytes key_package_bytes_tls_serialized = 1;
    bool is_inbox_id_credential = 2;
  }

  repeated KeyPackage key_packages = 1;
}

// Validates a Inbox-ID Key Package Type
message ValidateInboxIdKeyPackagesResponse {
  // one response corresponding to information about one key package
  message Response {
    bool is_ok = 1;
    string error_message = 2;
    xmtp.identity.MlsCredential credential = 3;
    bytes installation_public_key = 4;
    uint64 expiration = 5;
  }

  repeated Response responses = 1;
}

// Contains a batch of serialized Key Packages
message ValidateKeyPackagesRequest {
  // Wrapper for each key package
  message KeyPackage {
    bytes key_package_bytes_tls_serialized = 1;
    bool is_inbox_id_credential = 2;
  }

  repeated KeyPackage key_packages = 1;
}

// Response to ValidateKeyPackagesRequest
message ValidateKeyPackagesResponse {
  // An individual response to one key package
  message ValidationResponse {
    bool is_ok = 1;
    string error_message = 2;
    bytes installation_id = 3;
    string account_address = 4;
    bytes credential_identity_bytes = 5;
    uint64 expiration = 6;
  }

  repeated ValidationResponse responses = 1;
}

// Contains a batch of serialized Group Messages
message ValidateGroupMessagesRequest {
  // Wrapper for each message
  message GroupMessage { bytes group_message_bytes_tls_serialized = 1; }

  repeated GroupMessage group_messages = 1;
}

// Response to ValidateGroupMessagesRequest
message ValidateGroupMessagesResponse {
  // An individual response to one message
  message ValidationResponse {
    bool is_ok = 1;
    string error_message = 2;
    string group_id = 3;
    bool is_commit = 4;
  }

  repeated ValidationResponse responses = 1;
}

// Request to get a final association state for identity updates
message GetAssociationStateRequest {
  // List of identity updates
  repeated xmtp.identity.associations.IdentityUpdate old_updates = 1;
  repeated xmtp.identity.associations.IdentityUpdate new_updates = 2;
}

// Response to GetAssociationStateRequest, containing the final association
// state for an InboxID
message GetAssociationStateResponse {
  xmtp.identity.associations.AssociationState association_state = 1;
  xmtp.identity.associations.AssociationStateDiff state_diff = 2;
}
```

`ValidateKeyPackagesRequest` is used by exactly one rpc:
`ValidateInboxIdKeyPackages` takes `ValidateKeyPackagesRequest` but returns
`ValidateInboxIdKeyPackagesResponse` (`mls_validation/v1/service.proto:24-25`).
There is no second rpc that takes it.

Two messages in this file are dead — defined but referenced by no rpc:

- `ValidateInboxIdKeyPackagesRequest` — no rpc takes it.
- `ValidateKeyPackagesResponse` (`mls_validation/v1/service.proto:71`) — no rpc
  returns it, because the one rpc that takes the matching request returns
  `ValidateInboxIdKeyPackagesResponse` instead.

The pair is a half-finished rename: the request kept the old name and the
response got the new one, leaving one orphan on each side.

Note `ValidateGroupMessagesResponse.ValidationResponse.group_id` is a `string`
here, while every other group id in the repo is `bytes` — see §3.1.

The whole service exists because the Go node cannot parse MLS. A Rust
self-hosted backend can call the same libxmtp code in-process and delete this
package and the `apps/mls_validation_service` binary.

### 2.27 `xmtp.migration.api.v1`

`migration/api/v1/migration.proto` in full:

```proto
// V3 -> D14n Migration Protos
// These can be removed once migration is complete
syntax = "proto3";
package xmtp.migration.api.v1;

import "google/api/annotations.proto";
import "google/protobuf/empty.proto";

option go_package = "github.com/xmtp/proto/v3/go/migration/api/v1";
option java_package = "org.xmtp.proto.migration.api.v1";

service D14nMigrationApi {
  rpc FetchD14nCutover(google.protobuf.Empty) returns (FetchD14nCutoverResponse) {
    option (google.api.http) = {
      post: "/mls/v2/payer/fetch-d14n-cutover"
      body: "*"
    };
  }
}

message FetchD14nCutoverResponse {
  // the unix timestamp at which point d14n becomes the canonical backend
  uint64 timestamp_ns = 1;
}
```

The file's own header says it can be removed once migration is complete. The
self-hosted move makes the d14n cutover moot, so this package dies with it.
Call sites: `crates/xmtp_api_d14n/src/endpoints/d14n/fetch_d14n_cutover.rs`,
`crates/xmtp_api_d14n/src/queries/combined/tests.rs`,
`apps/xnet/lib/src/app/run.rs`.

---

## 3. Field-format notes

### 3.1 Hex strings vs raw bytes

The repo is not internally consistent; the rule is per field, not per type.

| Concept | Wire type | Encoding | Evidence |
| --- | --- | --- | --- |
| **inbox id** | `string` | **lowercase hex**, 64 chars (32 bytes) | `IdentityUpdate.inbox_id` (`association.proto:103`); `GetIdentityUpdatesRequest.Request.inbox_id`; `GetInboxIdsResponse.Response.inbox_id`; `MlsCredential.inbox_id`. libxmtp hex-decodes it before topic derivation: `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs:159` `hex::decode(&update.inbox_id)?`, and hex-encodes to build the request: `crates/xmtp_api_d14n/src/queries/v3/xmtp_query.rs:63` `inbox_id: hex::encode(topic.identifier())`. |
| **installation id / installation key** | `bytes` | **raw**, 32 bytes (Ed25519 public key) | `WelcomeMessageInput.V1.installation_key`, `RegisterInstallationResponse.installation_key`, `FetchKeyPackagesRequest.installation_keys`, `MemberIdentifier.installation_public_key`. libxmtp's `InstallationId` is a raw 32-byte array (`crates/xmtp_proto/src/types/topic.rs:11` comment: "the max size of an item in a `TopicKind` is 32 bytes (installation id)"). |
| **group id** | `bytes` | **raw** MLS group id | `GroupMessage.V1.group_id`, `QueryGroupMessagesRequest.group_id`, `PublishCommitLogRequest.group_id`, `PlaintextCommitLogEntry.group_id`. |
| **group id — the one exception** | `string` | **hex** | `xmtp.mls_validation.v1.ValidateGroupMessagesResponse.ValidationResponse.group_id` is `string`. Every other group id in the repo is `bytes`. |
| **topic** | `bytes` | **raw**, kind-prefixed (§3.2) | `AuthenticatedData.target_topic`, `EnvelopesQuery.topics`, `SubscribeTopicsRequest.TopicFilter.topic`, all XIP-83 `Subscription.topic` and `TopicsLive.topics`. backend.proto wraps it as `message Topic { bytes topic = 1 }`. |
| **v2 content topic** | `string` | human-readable, e.g. `/xmtp/0/...` | `xmtp.message_api.v1.Envelope.content_topic` — the v2 model, unrelated to the binary topics above. |
| **ethereum address** | `string` | `0x`-prefixed hex, EIP-55 or lowercase | `MemberIdentifier.ethereum_address`, `CreateInbox.initial_identifier`, `GetInboxIdsRequest.Request.identifier`. |
| **CAIP-10 account id** | `string` | `eip155:<chain>:<address>` | `SmartContractWalletSignature.account_id`, `VerifySmartContractWalletSignatureRequestSignature.account_id`. Comment cites the CAIP-10 spec. |
| **signature bytes** | `bytes` | raw, fixed lengths documented in comments | `RecoverableEcdsaSignature.bytes` is "65-bytes [ R \|\| S \|\| V ], with recovery id as the last byte"; `RecoverableEd25519Signature.bytes` is "64 bytes [R(32 bytes) \|\| S(32 bytes)]" and `.public_key` is "32 bytes". |
| **serialized sub-message** | `bytes` | raw protobuf or TLS | `PayerEnvelope.unsigned_client_envelope` (protobuf), `OriginatorEnvelope.unsigned_originator_envelope` (protobuf), `SignedPublicKey.key_bytes` (protobuf), `KeyPackageUpload.key_package_tls_serialized` (MLS TLS), `GroupMessageInput.V1.data` (MLS TLS), `CommitLogEntry.serialized_commit_log_entry` (protobuf). |
| **passkey key** | `bytes` | raw | `Passkey.key`, `RecoverablePasskeySignature.public_key`. |

The libxmtp `Topic` type serializes to hex in JSON, but that is a serde
convenience, not a wire format:
`crates/xmtp_proto/src/types/topic.rs:78-80` — `serializer.serialize_str(&hex::encode(...))`.

### 3.2 Topic derivation

Topics are **kind-prefixed binary**: one byte of kind, then the identifier.
Defined in XIP-49 §3.3.2 and implemented at
`/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/src/types/topic.rs:17-23`:

```rust
pub enum TopicKind {
    GroupMessagesV1 = 0,
    WelcomeMessagesV1 = 1,
    IdentityUpdatesV1 = 2,
    KeyPackagesV1 = 3,
}
```

The mapping from a `ClientEnvelope` payload to its topic is
`crates/xmtp_api_d14n/src/protocol/extractors/topics.rs`:

| Payload | Kind byte | Identifier | Source |
| --- | --- | --- | --- |
| `GroupMessageInput.V1` | `0x00` | the MLS group id, obtained by TLS-deserializing `data` into an `MlsMessageIn` → `ProtocolMessage` → `group_id()` | `topics.rs:86-92` |
| `GroupMessage.V1` (v3 response) | `0x00` | `message.group_id` directly — "The v3 response shapes carry their topic identifier as a plain field — no MLS deserialization needed, unlike the input shapes above." | `topics.rs:94-99` |
| `WelcomeMessageInput.V1` | `0x01` | `message.installation_key` | `topics.rs:124-127` |
| `WelcomeMessageInput.WelcomePointer` | `0x01` | `message.installation_key` | `topics.rs:129-135` |
| `WelcomeMessage.V1` / `.WelcomePointer` (v3 response) | `0x01` | `message.installation_key` | `topics.rs:100-108` |
| `IdentityUpdate` | `0x02` | `hex::decode(update.inbox_id)` — the decoded 32 bytes, **not** the 64 UTF-8 hex characters | `topics.rs:158-162` |
| `GetIdentityUpdatesRequest.Request` | `0x02` | `hex::decode(update.inbox_id)` | `topics.rs:164-170` |
| `UploadKeyPackageRequest` | `0x03` | TLS-deserialize `key_package.key_package_tls_serialized` into a `KeyPackageIn`, validate it, then take `kp.leaf_node().signature_key()` (the installation id) | `topics.rs:136-156` |

Two payload kinds therefore require full MLS parsing to compute their own topic:
group messages and key packages. That is a real cost the self-hosted backend
inherits if it derives topics server-side rather than trusting a client-supplied
topic. The `Topic::new_identity_update` doc comment states the hex trap
explicitly (`topic.rs:106-108`):

> this function expects the decoded hex from an InboxId, not the UTF-8 bytes of a InboxId.

### 3.3 AuthenticatedData

`xmtpv4/envelopes/envelopes.proto:18-24`, reproduced in full because it is small
and load-bearing:

```proto
// Data visible to the server that has been authenticated by the client.
message AuthenticatedData {
  // Do NOT reuse tag 1 — previously used by target_originator
  bytes target_topic = 2;
  Cursor depends_on = 3;
  // Do NOT reuse tag 4 — previously used by is_commit
}
```

It carries exactly two things at commit `dedb872`:

1. **`target_topic`** (`bytes`, tag 2) — the kind-prefixed binary topic (§3.2).
   This is the payload's routing key, made visible to the node without the node
   parsing the encrypted payload.
2. **`depends_on`** (`Cursor`, tag 3) — a vector clock naming the last envelopes
   this one causally depends on. Lets a node refuse to serve an envelope before
   its dependencies.

**`is_commit` is NOT in `AuthenticatedData`.** It was removed; tag 4 is
tombstoned by comment. The commit flag now lives on the server-populated
`GroupMessage.V1.is_commit` and in `mls_validation`'s validation response.
`target_originator` (tag 1) was likewise removed and moved to
`PayerEnvelope.target_originator`.

libxmtp constructs it in two shapes:

- `crates/xmtp_proto/src/types/topic.rs:272-278`, `AuthenticatedData::with_topic(topic)` — sets `target_topic`, leaves `depends_on: None`.
- `crates/xmtp_api_d14n/src/protocol/traits/envelopes.rs:139-144` — sets both, from a `TopicExtractor` and a `DependsOnExtractor` run in one pass over the payload.

backend.proto has **no `AuthenticatedData` at all**. Its `ClientEnvelope` is just
the payload oneof, and topic lives on the server-side `EnvelopeMeta.topic`. That
is a real semantic change: the client no longer asserts the topic; the server
derives it. See §5-Q2 and §5-Q8.

### 3.4 Cursors

Four distinct cursor notions coexist:

| Cursor | Shape | Where |
| --- | --- | --- |
| v2 `xmtp.message_api.v1.Cursor` | `oneof { IndexCursor index }`, `IndexCursor { bytes digest; uint64 sender_time_ns }` | `message_api/v1/message_api.proto:75-83` |
| v3 `id_cursor` | bare `uint64` inside `PagingInfo` / `Filter` / XIP-83 `Subscription` | `mls/api/v1/mls.proto:366, 398, 483` |
| v4 `xmtp.xmtpv4.envelopes.Cursor` | `map<uint32, uint64> node_id_to_sequence_id` — a vector clock. "The last seen entry per originator. Originators that have not been seen are omitted." | `xmtpv4/envelopes/envelopes.proto:13-16` |
| backend `xmtp.backend.v1.Cursor` | `uint64 sequence_id` | `docs/self-hosted/backend.proto` |

The self-hosted backend is single-originator, so collapsing the vector clock back
to a scalar `sequence_id` is correct. libxmtp already has a `GlobalCursor`
abstraction that bridges the v3 scalar and the v4 vector
(`crates/xmtp_proto/src/types/global_cursor.rs`, and `c.v3_message()` /
`c.v3_welcome()` / `c.inbox_log()` accessors in
`crates/xmtp_api_d14n/src/queries/v3/xmtp_query.rs:33, 47, 63`). Note it exposes
**three separate scalar cursors** per topic kind, so "one sequence_id" is a real
simplification the client side must absorb.

---

## 4. Proto build tooling

### 4.1 Proto repo tooling

**buf** — `/Users/nickmolnar/code/xmtp/proto/proto/buf.yaml` (v1 config, module
`buf.build/xmtp/proto`):

```yaml
version: v1
name: buf.build/xmtp/proto
deps:
  - buf.build/googleapis/googleapis
  - buf.build/grpc-ecosystem/grpc-gateway
breaking:
  use:
    - FILE
lint:
  use:
    - DEFAULT
  ignore_only:
    ENUM_ZERO_VALUE_SUFFIX:
      - message_contents/content.proto
    RPC_REQUEST_RESPONSE_UNIQUE:
      - message_api/v1/message_api.proto
      - identity/api/v1/identity.proto
      - mls/api/v1/mls.proto
      - xmtpv4/message_api/message_api.proto
      - xmtpv4/message_api/query_api.proto
      - xmtpv4/message_api/publish_api.proto
      - xmtpv4/message_api/notification_api.proto
      - xmtpv4/payer_api/payer_api.proto
      - xmtpv4/gateway_api/gateway_api.proto
    RPC_RESPONSE_STANDARD_NAME:
      - message_api/v1/message_api.proto
      - identity/api/v1/identity.proto
      - mls/api/v1/mls.proto
      - xmtpv4/message_api/notification_api.proto
  except:
    - PACKAGE_DIRECTORY_MATCH
    - PACKAGE_VERSION_SUFFIX
    - SERVICE_SUFFIX
```

Three global lint exceptions matter for the port:

- `PACKAGE_DIRECTORY_MATCH` off — `identity/associations/*.proto` has package
  `xmtp.identity.associations` but sits at `identity/associations/`, missing the
  `xmtp.` prefix directory. Every file in the repo has this shape.
- `PACKAGE_VERSION_SUFFIX` off — `xmtp.mls.message_contents`,
  `xmtp.identity.associations`, `xmtp.xmtpv4.envelopes` have no `.v1`.
  `xmtp.backend.v1` in backend.proto does, so it would pass this rule.
- `SERVICE_SUFFIX` off — services are `MlsApi`, `QueryApi`, not `MlsService`.
  backend.proto uses `QueryService`/`PublishService`/`SubscriptionService`/
  `IdentityService`, which **would** satisfy the default rule.

There is **no `buf.gen.yaml` in the proto repo**. Generation is per-consumer.

**protolint** — `/Users/nickmolnar/code/xmtp/proto/.protolint.yaml` (189 lines,
`all_default: true` with a long customization block).
`/Users/nickmolnar/code/xmtp/proto/dev/lint` installs it via
`go install github.com/yoheimuta/protolint/cmd/protolint@latest` and runs
`protolint lint -fix -config_path=./.protolint.yaml ./proto`.
One in-file directive exists: `message_api/v1/message_api.proto:34`
`// protolint:disable:next RPC_REQUEST_STANDARD_NAME`.

**Kotlin generation** — `/Users/nickmolnar/code/xmtp/proto/dev/kotlin/generate`
(1.7 KB), driven by Gradle (`build.gradle`, `settings.gradle`, `gradlew`,
`gradle/`), output in `kotlin/`. To be dropped.

**TypeScript generation** — `/Users/nickmolnar/code/xmtp/proto/dev/ts/generate`
(2.5 KB) plus `dev/ts/clean` and a vendored `dev/ts/protoc/`. Driven from
`package.json` (`npm run generate` → `prebuild`), compiled by `tsc` into
`ts/dist/{cjs,esm,types}`, published to npm as `@xmtp/proto` via
`semantic-release`. Runtime deps `protobufjs`, `long`, `rxjs`. To be dropped.
Note libxmtp's node/wasm bindings currently have `@xmtp/proto@npm:3.78.0` in
their yarn lockfiles
(`/Users/nickmolnar/code/xmtp/libxmtp/bindings/wasm/node_modules/.yarn-state.yml:254`,
`/Users/nickmolnar/code/xmtp/libxmtp/bindings/node/node_modules/.yarn-state.yml:435`)
— a transitive dep, worth confirming who pulls it.

**Proto repo CI** — `/Users/nickmolnar/code/xmtp/proto/.github/workflows/buf.yml`
runs `buf push proto --error-format github-actions --create --git-metadata` on
push to `main` (publishes the BSR module). `lint.yml`, `release.yml`,
`test.yml`, `triage.yml` cover protolint, semantic-release, and the TS build.

### 4.2 libxmtp's `xmtp_proto` build

**Generated code is checked in.** `crates/xmtp_proto/src/gen/` holds 46 `.rs`
files (~2.6 MB total) plus `proto_descriptor.bin` (381 KB). A normal
`cargo build` reads them and runs no protoc.

`crates/xmtp_proto/build.rs` is a no-op unless `GEN_PROTOS` is `true` or `1`:

```rust
let update = std::env::var("GEN_PROTOS");
let should_update = matches!(update, Ok(s) if s == "true" || s == "1");
if !should_update {
    return Ok(());
}
if !cmd_exists("protoc") {
    panic!("xmtp_proto buildscript requires protoc on $PATH");
}
```

When regeneration is on, `build.rs` (`clone_proto_repos`, lines 51-81):

1. `git clone https://github.com/grpc-ecosystem/grpc-gateway.git` into `$OUT_DIR`
   (skipped if present).
2. `git clone https://github.com/googleapis/googleapis.git` into `$OUT_DIR`
   (skipped if present).
3. Deletes and re-clones `https://github.com/xmtp/proto.git` into `$OUT_DIR/proto`.
4. `git checkout <revision>` where the revision comes from
   `crates/xmtp_proto/proto_version` — a single line, currently
   `dedb87251f23bee8133154706afbc0aa1348210d`.

Include paths (`build.rs:88-93`): `$OUT_DIR/proto/proto`, `$OUT_DIR/grpc-gateway/`,
`$OUT_DIR/grpc-gateway/third_party/googleapis/`, `$OUT_DIR/googleapis/`.

File discovery is a `WalkDir` over the whole cloned `proto/proto` tree,
`.sort()`ed with this comment (`build.rs:106-109`):

> prost emits each package file in input order, and WalkDir yields raw readdir
> order — filesystem- and machine-dependent. Sort, or every regen on a different
> machine rewrites unchanged generated files.

Codegen stack (`build.rs:136-160`):

- `tonic-prost-build = "0.14"` (build-dep) driving `protoc`.
- `Config::enable_type_names()`.
- `.compile_well_known_types(true)` with `.extern_path(".google.protobuf", "::pbjson_types")`.
- `.out_dir("src/gen")` — writes into the source tree, not `$OUT_DIR`.
- `.file_descriptor_set_path("src/gen/proto_descriptor.bin")`.
- `.build_client(false)` — **no tonic clients are generated**; libxmtp hand-writes
  its transport in `xmtp_api_grpc` / `xmtp_api_d14n`.
- Servers **are** generated, gated per package by `server_mod_attribute` with
  `#[cfg(any(not(target_arch = "wasm32"), feature = "grpc_server_impls"))]`
  (`codegen_configure`, `build.rs:10-49`) for: `xmtp.identity.api.v1`,
  `xmtp.mls_validation.v1`, `xmtp.message_api.v1`, `xmtp.mls.api.v1`,
  `xmtp.xmtpv4`, `xmtp.xmtpv4.payer_api`, `xmtp.xmtpv4.message_api`,
  `xmtp.xmtpv4.metadata_api`, `xmtp.migration.api.v1`.
- Then `pbjson_build::Builder` re-reads the descriptor set and emits
  `*.serde.rs` with `.ignore_unknown_fields()` and `.preserve_proto_field_names()`,
  scoped to `&[".xmtp"]`.

Runtime deps that the generated code needs (`crates/xmtp_proto/Cargo.toml`):
`prost` (with `derive`), `prost-types = "0.14"`, `tonic` (workspace),
`tonic-prost = "0.14"`, `pbjson`, `pbjson-types`, `serde`. On native,
`tonic` additionally needs features `channel` and `codegen`, with this comment:

> `codegen` is required because `build.rs` gates server-side generated modules
> on `cfg(any(not(target_arch = "wasm32"), feature = "grpc_server_impls"))`,
> so native builds always compile server stubs that `use tonic::codegen::*`.

`crates/xmtp_proto/src/gen/mod.rs` is **hand-maintained**. `build.rs` has the
`.include_file("mod.rs")` line commented out, with a note in
`crates/xmtp_proto/src/lib.rs:4-7`:

> Edit the 'build.rs' file and uncomment '.include_file' to generate this file
> from the beginning. Generating this file anew will remove all ".serde.rs"
> includes, since pbjson does not integrate with prost/tonic build

This is why `xmtp.xmtpv4.gateway_api` is generated but absent from `mod.rs`:
nobody added the `include!` line by hand.

**Adding it is not a one-line fix, and it would break the wasm build.** Two
reasons:

1. **No serde file to include.** `xmtpv4/gateway_api/gateway_api.proto` defines
   a service and no messages — it imports its request/response types from
   `xmtpv4/payer_api/payer_api.proto`. pbjson generates nothing for it, so
   there is no `xmtp.xmtpv4.gateway_api.serde.rs`. Only one `include!` exists
   to add, not the two every other package gets.
2. **No `cfg` gate on its server module.** The generated file
   (`crates/xmtp_proto/src/gen/xmtp.xmtpv4.gateway_api.rs:2`) opens straight
   into `pub mod gateway_api_server { ... }` with no attribute above it, and
   that module `use`s `tonic::codegen::*`. The `server_mod_attribute` list in
   `crates/xmtp_proto/build.rs:11-47` has an entry for `xmtp.xmtpv4` and one
   for `xmtp.xmtpv4.payer_api`, but **none for `xmtp.xmtpv4.gateway_api`** —
   prost matches these by exact package name, not by prefix, so the parent
   entry does not cover the child. Every other server module carries
   `#[cfg(any(not(target_arch = "wasm32"), feature = "grpc_server_impls"))]`.
   `tonic`'s `codegen` feature is a native-only dependency
   (`crates/xmtp_proto/Cargo.toml:45-49`), so an ungated `include!` fails to
   compile for `wasm32`.

Two clean options: either add an exact
`server_mod_attribute("xmtp.xmtpv4.gateway_api", ...)` to `build.rs`,
regenerate, and add the single `include!`; or delete the orphan generated file
and drop `gateway_api.proto` from the retained set. Do not just add the
`include!`.

`crates/xmtp_proto/buf.gen.yaml` (2.2 KB) is a **parallel, stale** buf-based
recipe for the same generation (`neoeinstein-prost`, `neoeinstein-prost-serde`,
`neoeinstein-tonic`, `prost-crate`). It is not referenced by `build.rs`,
`dev/gen_protos.sh`, or any CI workflow — it is the pre-`build.rs` approach left
behind. Its `server_mod_attribute` entries lack the `grpc_server_impls` feature
gate that `build.rs` has, confirming it is out of date.

### 4.3 Exact libxmtp files that reference the proto repo

| Path | What it does |
| --- | --- |
| `/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/build.rs:72` | `git clone https://github.com/xmtp/proto.git {out_dir}/proto` |
| `/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/build.rs:60,66` | clones `grpc-ecosystem/grpc-gateway` and `googleapis/googleapis` |
| `/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/build.rs:76-79` | `git checkout` of the pinned revision |
| `/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/proto_version` | the pin: one line, `dedb87251f23bee8133154706afbc0aa1348210d` |
| `/Users/nickmolnar/code/xmtp/libxmtp/dev/gen_protos.sh:10` | `REV=$(git ls-remote https://github.com/xmtp/proto "$BRANCH" \| awk '{print $1}')` — resolves a branch to a SHA, writes it to `proto_version`, then runs `GEN_PROTOS=1 cargo build -p xmtp_proto --features grpc_server_impls` |
| `/Users/nickmolnar/code/xmtp/libxmtp/.github/workflows/nightly-protos.yml` | Weekly (`cron: '0 10 * * 1'`) + `workflow_dispatch`. Installs buf (`bufbuild/buf-setup-action@v1.50.0`) and `protobuf-compiler` via apt, runs `dev/gen_protos.sh`, runs `taplo format`, then opens a PR on branch `nightly-proto` titled "Update Protos". |
| `/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/buf.gen.yaml` | stale parallel buf recipe (§4.2) |
| `/Users/nickmolnar/code/xmtp/libxmtp/nix/lib/shell-common.nix:21,132` | `protobuf` in the dev shell inputs and package list — this is what puts `protoc` on `$PATH` |
| `/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/README.md:5` | Tells the reader to run `../dev/gen_protos.sh` and commit the result. It must be rewritten with that script (§4.4 item 4). |
| `/Users/nickmolnar/code/xmtp/libxmtp/bindings/node/yarn.lock:1923` | tracked lockfile — pins `"@xmtp/proto": "npm:3.78.0"` as a transitive dependency of `@xmtp/content-type-primitives` |
| `/Users/nickmolnar/code/xmtp/libxmtp/bindings/wasm/yarn.lock:854` | tracked lockfile — same `"@xmtp/proto": "npm:3.78.0"` transitive pin |
| `.../bindings/*/node_modules/.yarn-state.yml` | generated install state, **not** lockfiles and **not** git-tracked. They mirror the two `yarn.lock` files above. Ignore them. |

The npm `@xmtp/proto` package is the JavaScript codegen of the same proto repo.
It reaches the bindings only through `@xmtp/content-type-primitives`, a
JS-side dependency of the test/example code. It is not part of the Rust build
and the in-repo `proto/` move does not change it.

**There is no git submodule and no nix flake input for the proto repo.** The
only link is the runtime `git clone` in `build.rs` plus the SHA in
`proto_version`. `grep -n "proto" flake.nix` returns nothing.

### 4.4 What Phase 1 needs to change

To make an in-repo `proto/` folder authoritative:

1. Create `/Users/nickmolnar/code/xmtp/libxmtp/proto/` with the retained `.proto`
   files, preserving the directory layout the `import` statements expect
   (`identity/associations/signature.proto`, `mls/api/v1/mls.proto`, etc.), so no
   import paths change.
2. In `crates/xmtp_proto/build.rs`: delete `clone_proto_repos` and the
   `xshell` build-dep; point the `WalkDir` and the include path at the in-repo
   folder; keep the `.sort()`.
3. Decide the fate of the two external include paths (`grpc-gateway`,
   `googleapis`). They exist only for `google/api/annotations.proto` and
   `protoc-gen-openapiv2/options/annotations.proto`, both used purely for HTTP
   gateway annotations. Dropping the annotations from the retained files removes
   both clones. Otherwise vendor the two annotation files.
4. Delete `crates/xmtp_proto/proto_version` and `dev/gen_protos.sh`, or repoint
   `gen_protos.sh` at the local folder (still worth keeping as the "regenerate"
   entry point). Either way, rewrite `crates/xmtp_proto/README.md:3-5` — it
   points the reader at `github.com/xmtp/proto` and tells them to run
   `../dev/gen_protos.sh` to pull upstream changes. Both statements become
   wrong the moment the protos live in-repo.
5. Delete or rewrite `.github/workflows/nightly-protos.yml` — with the protos
   in-repo, the nightly sync PR has no upstream to sync from. It should become a
   CI check that `src/gen/` is up to date with `proto/`, not a PR opener.
6. Delete the stale `crates/xmtp_proto/buf.gen.yaml`.
7. Resolve the orphan `xmtp.xmtpv4.gateway_api`. Right now it is generated on
   disk and not included, so it compiles nowhere. **Do not simply add an
   `include!` to `src/gen/mod.rs`** — that breaks the wasm build, because the
   generated server module has no `cfg` gate and `build.rs` has no exact
   `server_mod_attribute` entry for that package (see §4.2). There is also no
   serde file for it, since the proto declares a service and no messages.
   Either add the exact `server_mod_attribute("xmtp.xmtpv4.gateway_api", ...)`
   and then the one `include!`, or — the simpler choice, given zero call sites
   — delete both the generated file and the proto.
8. `protoc` must stay available for regeneration only.
   `nix/lib/shell-common.nix` already provides it. Normal builds do not need it.
9. `buf` becomes optional: nothing in the libxmtp build calls it. Keeping a
   `buf.yaml` in the new folder for lint/breaking-change checks is cheap and
   would carry over the ignore rules in §4.1.

---

## 5. Open questions and inconsistencies

### Q1. `GetInboxIdsRequest` exists in two packages with identical shape; backend.proto qualifies neither

`docs/self-hosted/backend.proto` ends with:

```proto
service IdentityService {
  rpc GetInboxIds(GetInboxIdsRequest) returns (GetInboxIdsResponse) {};
  rpc VerifySmartContractWalletSignatures(VerifySmartContractWalletSignaturesRequest) returns (VerifySmartContractWalletSignaturesResponse) {};
}
```

All four type names are **unqualified**, so protoc resolves them in
`xmtp.backend.v1` — where they do not exist. The file will not compile as
written. There are two existing candidates for `GetInboxIds*`, field-identical:

- `xmtp.identity.api.v1.GetInboxIdsRequest/Response` (`identity/api/v1/identity.proto:120-140`)
- `xmtp.xmtpv4.message_api.GetInboxIdsRequest/Response` (`xmtpv4/message_api/message_api.proto:239-259`)

Both nest `Request { string identifier = 1; IdentifierKind identifier_kind = 2 }`
and `Response { string identifier = 1; optional string inbox_id = 2;
IdentifierKind identifier_kind = 3 }`. They differ in nothing but package.
`VerifySmartContractWalletSignatures*` exists only in `xmtp.identity.api.v1`.

**Question:** does backend.proto intend to import `identity/api/v1/identity.proto`
and qualify these, or to redefine them in `xmtp.backend.v1`? Importing brings
the whole `IdentityApi` service and its grpc-gateway annotations along;
redefining is four small messages and drops the dependency, but forks the type.

### Q2. `ClientEnvelope` field numbering starts at 2 with tag 1 unused

`xmtp.backend.v1.ClientEnvelope`:

```proto
message ClientEnvelope {
  oneof payload {
    xmtp.mls.api.v1.GroupMessageInput group_message = 2;
    ...
  }
}
```

In v4, tag 1 was `AuthenticatedData aad`. backend.proto drops `aad` but keeps the
tag numbering, leaving tag 1 unused and undocumented. Two readings:

- **Deliberate** — wire compatibility with v4 `ClientEnvelope` bytes, so an
  existing serialized v4 envelope decodes as a backend envelope (minus the aad).
  If so, say it in a comment and add `reserved 1;` so nobody claims it.
- **Vestigial** — a copy-paste from v4 nobody renumbered. If so, renumber to
  start at 1.

Note that if the intent is v4 wire compatibility, then dropping
`payer_report = 6` / `payer_report_attestation = 7` and adding
`commit_log_entry = 6` **breaks** it: tag 6 changes meaning from `PayerReport`
to `CommitLogEntry`. A v4 payer report would decode as a malformed commit log
entry. Either reserve 6 and 7 and put `commit_log_entry` at 8, or accept that
compatibility is already gone and renumber from 1.

Also: v4 names the key-package arm `upload_key_package`; backend.proto renames it
`key_package` while keeping tag 4. The type is still
`xmtp.mls.api.v1.UploadKeyPackageRequest`, so a message named "Request" is being
used as a payload, not a request. Worth a rename to `KeyPackageUpload` (which
already exists, is the inner type, and is the more accurate name) — though that
would change the `is_inbox_id_credential` bool's home.

### Q3. No key-package read path, no installation registration/revocation

backend.proto's `IdentityService` has two rpcs. The v3 `MlsApi` had three more
identity-adjacent ones with no backend equivalent:

- `FetchKeyPackages(FetchKeyPackagesRequest) → FetchKeyPackagesResponse` — the
  read side of key packages. `ClientEnvelope.key_package` covers the write side
  (publish), and `TopicKind::KeyPackagesV1 = 3` exists, so a client could in
  principle `QueryNewest` a key-package topic. But `QueryNewest` returns a
  `ServerEnvelope` wrapping an `UploadKeyPackageRequest`, not a
  `FetchKeyPackagesResponse.KeyPackage`. Is querying the key-package topic the
  intended fetch path? If yes, the "consume one key package per fetch" semantics
  (each fetch should hand out an unused key package) do not map onto a
  read-only topic query — that is a stateful pop, not a read.
- `RegisterInstallation` — installations are now presumably registered by
  publishing a key package plus an identity update. Confirm.
- `RevokeInstallation` — revocation now presumably rides on
  `IdentityUpdate` → `RevokeAssociation`. If so, the v3 rpc and its
  `xmtp.message_contents.Signature` import can both go.

### Q4. Commit log: which of three near-identical shapes does `ClientEnvelope` carry?

backend.proto uses `xmtp.mls.message_contents.CommitLogEntry`:

```proto
message CommitLogEntry {
  uint64 sequence_id = 1;
  bytes serialized_commit_log_entry = 2;
  xmtp.identity.associations.RecoverableEd25519Signature signature = 3;
}
```

`sequence_id` is a **server-assigned** field — it is the entry's position in the
server's log. A client publishing a commit log entry cannot know it. The v3
publish shape is a different message precisely for this reason:

```proto
message PublishCommitLogRequest {
  bytes group_id = 1;
  bytes serialized_commit_log_entry = 2;
  xmtp.identity.associations.RecoverableEd25519Signature signature = 3;
}
```

`PublishCommitLogRequest` has `group_id` at the top level and no `sequence_id`.
`CommitLogEntry` has `sequence_id` and no top-level `group_id`.

**The topic is still derivable.** An earlier draft of this page claimed the
server cannot route a `CommitLogEntry` because it carries no `group_id`. That is
wrong. In production, `serialized_commit_log_entry` is a **plaintext, unencrypted
protobuf encoding of `PlaintextCommitLogEntry`**, and that message carries
`bytes group_id = 1` as its first field
(`mls/message_contents/commit_log.proto:17`). libxmtp writes it that way and
reads it back that way:

- encode: `crates/xmtp_mls/src/groups/commit_log.rs:393` —
  `let serialized_commit_log_entry = entry.encode_to_vec();` where `entry` is a
  `&PlaintextCommitLogEntry`. There is no encryption step in this path.
- decode: `crates/xmtp_mls/src/groups/commit_log.rs:499` —
  `PlaintextCommitLogEntry::decode(commit_log_entry.serialized_commit_log_entry.as_slice())`.

So a server holding a `CommitLogEntry` can decode one inner protobuf and read
`group_id`. That is a cheap parse, far cheaper than the MLS TLS deserialize a
group message needs (§3.2). The "private commit log" the proto comment hints at
has no implementation today; if an encrypted variant is ever added, this
derivation breaks and the question reopens.

**What is still wrong with `CommitLogEntry` as a `ClientEnvelope` payload:**
the client must send a meaningless `sequence_id` (0?), and the server must
ignore it. `sequence_id` is **server-assigned** — the entry's position in the
server's log — so the same field is write-ignored on publish and authoritative
on read. That is a real defect, just a smaller one than a routing failure.

`PublishCommitLogRequest` remains the cleaner publish shape: it has no
`sequence_id` to ignore, and its explicit `group_id` makes routing free instead
of merely cheap. Recommended fix: use `PublishCommitLogRequest` as the
`ClientEnvelope` payload type and keep `CommitLogEntry` for the read path,
which is exactly the v3 split.

### Q5. Two different messages named `Signature`

`xmtp.message_contents.Signature` (v2 — `ECDSACompact` / `WalletECDSACompact`
oneof, 64-byte compact + `uint32 recovery`) and
`xmtp.identity.associations.Signature` (the five-arm union). They are not
interchangeable and are both reachable from `mls.proto`:
`RevokeInstallationRequest.wallet_signature` uses the v2 one,
everything identity-related uses the associations one. If `RevokeInstallation`
goes away (Q3), the v2 `Signature` is reachable only via
`LegacyDelegatedSignature` → `SignedPublicKey` → `Signature`, and can be
retired the day legacy v2 key delegation is dropped.

### Q6. `SortDirection` is defined twice and libxmtp imports the wrong one

Two identical enums:

- `xmtp.mls.api.v1.SortDirection` (`mls/api/v1/mls.proto:356-360`)
- `xmtp.message_api.v1.SortDirection` (`message_api/v1/message_api.proto:60-64`)

Same three values, same numbers. `xmtp.mls.api.v1.PagingInfo.direction` is typed
as the **mls** one. But `crates/xmtp_mls/src/groups/commit_log.rs:39` imports
`xmtp::message_api::v1::SortDirection`, and
`crates/xmtp_api/src/mls.rs:760` writes
`direction: xmtp_proto::xmtp::message_api::v1::SortDirection::Ascending as i32`
into an `mls_v1::PagingInfo`. It works only because both enums have identical
numeric values and prost represents enum fields as `i32`. Fix both to
`mls_v1::SortDirection`, as
`crates/xmtp_api_d14n/src/queries/v3/xmtp_query.rs:12` already does.

**That fix alone does not free the package.** These two imports are not the only
non-test use: `crates/xmtp_proto/src/api_client.rs:1-4` publicly re-exports eight
v2 types (`BatchQueryRequest`, `BatchQueryResponse`, `Envelope`, `PublishRequest`,
`PublishResponse`, `QueryRequest`, `QueryResponse`, `SubscribeRequest`), which
reach the crate's public API through `pub use generated::*` in
`crates/xmtp_proto/src/lib.rs:22`. Three things must change before
`message_api.proto` can be dropped: this re-export, the two `SortDirection`
references above, and the test import of `PublishRequest`. See §1.4 item 5.

### Q7. `WelcomeMessageInput.V1.welcome_metadata` is tag 7, with 5 and 6 silently skipped

```proto
message V1 {
  bytes installation_key = 1;
  bytes data = 2;
  bytes hpke_public_key = 3;
  xmtp.mls.message_contents.WelcomeWrapperAlgorithm wrapper_algorithm = 4;
  bytes welcome_metadata = 7;
}
```

Tags 5 and 6 are skipped with no `reserved` statement and no comment. The
matching `WelcomeMessage.V1` uses 1..7 contiguously with `welcome_metadata = 7`,
which suggests the input struct was aligned to the output struct's tag numbers
after the fact. Harmless but worth a `reserved 5, 6;` when copying, so a future
editor does not reuse them.

Compare `AuthenticatedData`, which handles the same situation correctly with
`// Do NOT reuse tag 1` / `// Do NOT reuse tag 4` comments — though even there,
a real `reserved 1, 4;` would be enforced by the compiler while a comment is not.

### Q8. Does `EnvelopeMeta.topic` duplicate data already in the payload?

```proto
message EnvelopeMeta {
  Cursor cursor = 1;
  uint64 server_ns = 2;
  MessageHash message_hash = 3;
  Topic topic = 4;
  optional uint64 expiry_ns = 5;
}
```

For **all five** payload kinds, yes — the topic is derivable from the payload
(§3.2), commit log entries included. The commit-log case is the least obvious:
`CommitLogEntry` has no top-level `group_id`, but its
`serialized_commit_log_entry` is a plaintext `PlaintextCommitLogEntry` whose
first field is `bytes group_id = 1`, and libxmtp already encodes and decodes it
that way (`crates/xmtp_mls/src/groups/commit_log.rs:393` and `:499`) — see Q4.

But derivation is not free, and the cost is uneven: group messages need an MLS
TLS deserialize, key packages need a full key-package validation, while a commit
log entry needs only one inner protobuf decode. Carrying the topic in the meta
is the right call for read paths, since the client should not have to re-derive
it to demux a stream.

The open question is the **write** path. v4 solved it with
`AuthenticatedData.target_topic` — the client asserts the topic and the node
routes on that without parsing the payload. backend.proto has no such field, so
either:

- the server derives the topic on every publish, paying the MLS parse cost on
  group messages and the validation cost on key packages, or
- the topic should be added to the publish request.

Since `PublishResponse` returns `repeated EnvelopeMeta` (which includes `topic`),
the server clearly knows the topic by the end of a publish. What is undefined is
how it learned it. Worth an explicit decision: either document "server derives,
here is the derivation table" as normative, or add a client-supplied topic and
document that the server validates it against the payload.

Related: `MessageHash { oneof hash { bytes sha256 = 1 } }` — what is hashed?
The serialized `ClientEnvelope`, the payload, or the payload's inner blob?
Idempotent publish and dedupe both depend on the answer.

### Q9. Validation responsibility is unstated

The v3/v4 split put MLS parsing in a Rust sidecar (`mls_validation`, §2.26)
because the Go node could not do it. A Rust backend inside libxmtp can validate
in-process. backend.proto says nothing about what `Publish` validates:

- Is a `GroupMessageInput` parsed and checked (well-formed MLS, epoch, sender)?
- Is an `UploadKeyPackageRequest` validated (the current topic derivation
  already runs `KeyPackageIn::validate` with a lifetime policy —
  `topics.rs:145-150` — so at minimum the topic path implies validation)?
- Is an `IdentityUpdate` checked against association state before being accepted?
  That is `mls_validation.GetAssociationState`'s job today, and it needs the
  prior updates for the same inbox.

The answer determines whether `mls_validation/v1/service.proto` and
`apps/mls_validation_service` survive at all, and whether `identity/credential.proto`
and `identity/associations/association.proto`'s `AssociationState` /
`AssociationStateDiff` messages are needed by the backend.

### Q10. backend.proto has no package imports, no options, and unbounded/uncorrelated request shapes

Smaller items, grouped:

1. **No `import` statements.** The file references
   `xmtp.mls.api.v1.*`, `xmtp.identity.associations.IdentityUpdate`, and
   `xmtp.mls.message_contents.CommitLogEntry` without importing the files that
   define them. It cannot compile as-is. Needs at minimum
   `import "mls/api/v1/mls.proto";`,
   `import "identity/associations/association.proto";`,
   `import "mls/message_contents/commit_log.proto";`, plus whatever Q1 resolves to.
2. **No `go_package` / `java_package` options.** Fine if Rust-only, but the
   mobile bindings may care. Decide explicitly.
3. **`QueryNewestResponse` uses `repeated` inside a `oneof`** —

   ```proto
   message QueryNewestResponse {
     oneof response {
       repeated EnvelopeMeta envelope_metas = 1;
       repeated ServerEnvelope envelopes = 2;
     }
   }
   ```

   **This is illegal protobuf.** `oneof` fields cannot be `repeated`. It needs
   wrapper messages, exactly like `SubscribeResponse.Messages` does two messages
   later. The v4 equivalent (`SubscribeTopicsResponse`) gets this right by
   wrapping in `message Envelopes { repeated ... }`. Same bug shape as
   `SubscribeOriginatorsResponse` avoids.
4. **`QueryNewest` loses the "same length as request, nil for missing" contract.**
   Both v3 `GetNewestGroupMessageResponse` and v4 `GetNewestEnvelopeResponse`
   wrap each result in a `Response` message with an `optional` field, documented
   as "will always be the same length as the request" / "OR null if there are no
   envelopes on the topic". backend.proto's flat `repeated EnvelopeMeta` cannot
   express "topic 3 had nothing" — positional correlation is lost. Add the
   per-topic wrapper.
5. **`SubscribeOnceResponse` has no `Pong`.** It has `Ping` in the response
   oneof, but `SubscribeOnceRequest` has no request oneof at all — it is
   `repeated TopicQuery topics` only, and `SubscribeOnce` is
   `(SubscribeOnceRequest) returns (stream ...)`, i.e. server-streaming. A
   client cannot answer a `Ping` on a server-streaming rpc. Either drop the
   `Ping` arm or document it as a keepalive the client only observes (never
   answers), which is a different thing from the bidi `Ping` and should probably
   have a different name.
6. **`SubscribeOnceResponse.CatchupComplete` is empty** while
   `SubscribeResponse.CatchupComplete` carries `mutate_id`. Consistent given
   there are no mutates, but then the two messages sharing a name across two
   response types invites confusion. Consider `Done` or `Complete`.
7. **`QueryRequest.limit` is `uint32` with no documented default or maximum.**
   v4's `QueryEnvelopesRequest.limit` has the same gap; libxmtp supplies
   `xmtp_configuration::MAX_PAGE_SIZE` client-side
   (`crates/xmtp_api_d14n/src/queries/v3/xmtp_query.rs:35`). Document the
   server-side cap.
8. **`QueryResponse` has no paging cursor.** v3's `QueryGroupMessagesResponse`
   returns `PagingInfo` so the client can continue. backend.proto's
   `QueryResponse { repeated ServerEnvelope envelopes }` returns nothing to
   resume from. The client can take the last envelope's `meta.cursor`, which
   works for a single topic but is ambiguous for a multi-topic `QueryRequest`
   (which cursor advanced? did every topic reach its end, or did the limit cut
   one short?). Either return per-topic cursors or document that clients must
   re-issue with per-topic cursors derived from the returned envelopes.
9. **`Topic` is a wrapper message around one `bytes` field.** v3 and v4 both use
   bare `bytes` for topics. Wrapping costs 2 extra bytes per topic on the wire
   and forces an `Option<Topic>` in Rust for every topic field. It does buy
   type safety in generated code. Worth confirming it is deliberate — the same
   question applies to `Cursor { uint64 sequence_id }` and
   `MessageHash { oneof }`.
10. **`SubscribeRequest.Mutate.removes` is `repeated Topic`** while the v3 and v4
    XIP-83 versions use `repeated bytes`. Consistent with item 9, but note
    `adds` is `repeated TopicQuery` (which nests `Topic` + `Cursor`), so the two
    lists have different element shapes than their v3/v4 counterparts
    (`Subscription { bytes topic; uint64 id_cursor }` / `repeated bytes`).
11. **No `oneof version` wrapper.** Both existing XIP-83 bindings wrap
    `SubscribeRequest` / `SubscribeResponse` in `oneof version { V1 v1 = 1 }`
    and document the pinning rule ("a stream whose requests are V1 receives only
    V1 responses"). backend.proto drops the versioning. That is a deliberate
    simplification for a self-hosted single-implementation backend, but it means
    a future protocol revision has no in-band escape hatch. Worth a decision, not
    an accident.
12. **`ClientEnvelope` is unsigned.** v4 wrapped it in `PayerEnvelope` (payer
    signature) inside `OriginatorEnvelope` (node signature). backend.proto has
    neither. For self-hosted that is probably right — the transport is trusted —
    but it should be stated, since it means the server has no cryptographic proof
    of who published what, and clients have no proof the server did not fabricate
    an envelope.

---

## Review status

An adversarial review of this page ran against the two source repositories on
2026-09-02 (Codex, `gpt-5.6-sol`, read-only sandbox, high reasoning effort).
Review thread id: `01a06249-17ef-7163-b90e-160cf14b4738`.

The review returned `VERDICT: ISSUES` with nine findings: three `major`, six
`minor`, no blockers. Every finding was checked against the source files and
every one was correct. All nine are now applied to the page above.

| # | Finding | Applied or rejected | Note |
| --- | --- | --- | --- |
| 1 | §1.3, §1.4 item 5, Q6 — `xmtp.message_api.v1` has a non-test public re-export of eight v2 types, so fixing the two `SortDirection` references does not make the package removable (major) | **Applied** | Confirmed at `crates/xmtp_proto/src/api_client.rs:1-4` (`BatchQueryRequest`, `BatchQueryResponse`, `Envelope`, `PublishRequest`, `PublishResponse`, `QueryRequest`, `QueryResponse`, `SubscribeRequest`) reaching the public API via `pub use generated::*` at `crates/xmtp_proto/src/lib.rs:22`. §1.3 row, §1.4 item 5, and Q6 now list the three changes required before the package can go. `authn.proto` stays unused. |
| 2 | §1.4 item 4 — `message_contents/ciphertext.proto` is not independently removable, because the test-used `private_key.proto` imports it (major) | **Applied** | Confirmed: `proto/message_contents/private_key.proto:9` imports `ciphertext.proto`; `crates/xmtp_mls/src/test/builder.rs:25-30` uses `SignedPrivateKey` and `signed_private_key::{Secp256k1, Union}`. `ciphertext` removed from the "no call sites" list and given its own paragraph with the ordering constraint. |
| 3 | Q4 and Q8 — commit-log topic derivation *is* possible; `serialized_commit_log_entry` is a plaintext `PlaintextCommitLogEntry` carrying `group_id` (major) | **Applied** | Confirmed: encode at `crates/xmtp_mls/src/groups/commit_log.rs:393` (`entry.encode_to_vec()`, no encryption step), decode at `:499` (`PlaintextCommitLogEntry::decode`), field at `mls/message_contents/commit_log.proto:17`. Q4 rewritten to retract the routing claim while keeping the server-assigned `sequence_id` defect and the `PublishCommitLogRequest` recommendation. Q8 now says all five payload kinds are derivable, with uneven cost. §2.10 corrected too — it carried the same "encrypted blob" claim. |
| 4 | §4.2 and §4.4 item 7 — adding `gateway_api` to `gen/mod.rs` is unsafe for wasm, and there is no gateway serde file (major) | **Applied** | Confirmed: `crates/xmtp_proto/src/gen/xmtp.xmtpv4.gateway_api.rs:2` opens an ungated `pub mod gateway_api_server`; `crates/xmtp_proto/build.rs:11-47` has entries for `xmtp.xmtpv4` and `xmtp.xmtpv4.payer_api` but none for `xmtp.xmtpv4.gateway_api`, and prost matches these by exact package name, not prefix; `tonic`'s `codegen` feature is native-only (`crates/xmtp_proto/Cargo.toml:45-49`). The proto declares a service and zero messages, so pbjson emits no serde file. Both sections now give the two safe options and warn against the bare `include!`. |
| 5 | §1.2 row 57 — `misbehavior_api.proto` *is* generated into libxmtp, into the shared `xmtp.xmtpv4.message_api` output file (minor) | **Applied** | Confirmed at `crates/xmtp_proto/src/gen/xmtp.xmtpv4.message_api.rs:1431` (`UnsignedMisbehaviorReport`, `MisbehaviorReport`, `SubmitMisbehaviorReportRequest`). Row 57 and §1.4 item 6 now say the symbols are generated and compiled but have no hand-written call sites — dead by usage, not by generation. |
| 6 | §2.1 — the displayed dependency closure holds nine unique first-party files, not eight (minor) | **Applied** | Recount confirms nine: `mls/api/v1/mls.proto`, `identity/associations/signature.proto` (which imports `message_contents/public_key.proto` at `signature.proto:6`), `message_contents/public_key.proto`, `message_contents/signature.proto`, `mls/message_contents/commit_log.proto`, `welcome_pointer.proto`, `wrapper_encryption.proto`, `identity/associations/association.proto`, `identity/api/v1/identity.proto`. The count now names all nine so it cannot drift again. |
| 7 | §2.26 verbatim block — comment reads "Response to GetAssociationStateResponse"; the source says "Request" (minor) | **Applied** | Confirmed at `mls_validation/v1/service.proto:114`. Corrected in the transcription. |
| 8 | §2.26 dead-message note — `ValidateKeyPackagesRequest` is used by one rpc, not two, and `ValidateKeyPackagesResponse` is also dead (minor) | **Applied** | Confirmed: `mls_validation/v1/service.proto:24-25` shows `ValidateInboxIdKeyPackages` as the only rpc taking `ValidateKeyPackagesRequest`, and it returns `ValidateInboxIdKeyPackagesResponse`; `ValidateKeyPackagesResponse` at `:71` is returned by no rpc. The note now lists both orphans and explains the half-finished rename. |
| 9 | §4.3 — the list omits the tracked `yarn.lock` files, wrongly calls generated `.yarn-state.yml` files lockfiles, and omits the README that references the script proposed for deletion (minor) | **Applied** | Confirmed: `git ls-files` tracks `bindings/node/yarn.lock` and `bindings/wasm/yarn.lock`; `git ls-files` returns nothing for either `node_modules/.yarn-state.yml`. `@xmtp/proto@npm:3.78.0` appears at `bindings/node/yarn.lock:1923` and `bindings/wasm/yarn.lock:854`, in both cases as a transitive dependency of `@xmtp/content-type-primitives`. `crates/xmtp_proto/README.md:3-5` points at the upstream repo and `dev/gen_protos.sh`. §4.3 now lists both tracked locks and the README, and demotes the `.yarn-state.yml` files to generated state; §4.4 item 4 adds the README rewrite. |

No finding was rejected.

### Residual risk

The review verified all 62 inventory paths, 34 of 35 transcribed proto blocks,
the usage classification of at least 16 packages, and the build-tooling claims
(pinned commit, `GEN_PROTOS` gate, repo clones, sorted discovery, tonic/pbjson
config, `dev/gen_protos.sh`, the nightly workflow, and the hand-maintained
`gen/mod.rs`). Q1, Q2, Q3, Q5, Q7, Q9 and the Q10 protobuf/query defects were
confirmed correct. What remains uncertain is narrower but real. The "unused"
verdicts rest on grep over `.rs` files in this workspace, so a package reached
only through a macro, a string-built path, a feature-gated file the grep did not
expand, or a downstream consumer outside libxmtp (the mobile, wasm, and node
SDKs, and any external crate depending on `xmtp_proto`) would not appear —
finding 1 is exactly this failure mode caught once, and a public re-export is
precisely the shape that hides from a call-site grep, so treat every remaining
"zero hits" verdict as "no hits in this workspace" and run a compile before
deleting anything. Usage counts are point-in-time against proto commit
`dedb87251f23bee8133154706afbc0aa1348210d` and drift with every upstream sync.
The `gen/mod.rs` and `build.rs` claims describe today's hand-maintained state,
which changes the moment Phase 1 edits either file. The commit-log conclusion in
Q4 and Q8 holds only while `serialized_commit_log_entry` stays plaintext: an
encrypted private-commit-log variant would restore the routing problem this
revision retracted. Finally, the proto-to-topic derivation table in §3.2 is read
from `crates/xmtp_api_d14n/src/protocol/extractors/topics.rs`, which is d14n
code — a self-hosted backend that reimplements derivation must match it exactly,
including the hex-decode trap on inbox ids.
