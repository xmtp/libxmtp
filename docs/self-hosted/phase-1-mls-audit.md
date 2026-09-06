# Phase 1 MLS extraction audit

Status: identity extraction implemented on the Phase 1 identity branch.
Envelope validation, commit-log extraction, dependency isolation, and final test
traceability remain integration tasks. This document does not claim those tasks
are complete.

## Scope and method

Reviewed the 133 Rust source files under `crates/xmtp_mls/src` at the phase base, the benchmark
sources and archive assets, and the validation service handlers and cache.
The review used the file inventory, module declarations, function/type inventory,
imports, and focused reads of parsing, construction, signing, hashing, and test
helpers. Recursive categories below include every child module and inline test.

The governing requirements are [spec 001](../specs/001_backend_api.md),
[spec 002](../specs/002_backend_architecture.md), and
[spec 003](../specs/003_message_security.md). ARC-010 through ARC-013 require shared,
database-free validation and fixtures. SEC-002, SEC-010 through SEC-015, and
SEC-020 through SEC-044 define the extraction boundary. Pure code is not, by
itself, evidence that the backend needs it.

## Extracted identity code

Paths in the first column are the original source locations.

| Original source and symbol | Current shared owner | Client or service boundary |
| --- | --- | --- |
| `xmtp_mls/src/identity.rs::create_credential`, `parse_credential` | `xmtp_id/src/key_package/credential.rs` | Client wrappers only convert to `IdentityError`. No new inbox-format check. |
| `identity.rs::XmtpKeyPackageBuilder::build`, construction portion | `xmtp_id/src/key_package/construction.rs::build_key_package` | `KeyPackageOptions` supplies lifetime, PQ, and capability options. The client adapter stores references and history. |
| `identity.rs::build_post_quantum_public_key_extension` | `xmtp_id/src/key_package/construction.rs` | The client wrapper preserves its error type. |
| `identity.rs::generate_post_quantum_key`, `GeneratePostQuantumKeyError` | `xmtp_cryptography/src/post_quantum.rs` | `xmtp_common/src/error_code.rs` preserves the Crypto/Rand error labels. |
| `groups/mls_ext/mls_ext_welcome_pointee_encryption_aead_type.rs::WelcomePointersExtension` and conversions | `xmtp_id/src/key_package/welcome_pointers.rs` | The client module re-exports the shared type. |
| `identity_updates.rs::verify_updates` | `xmtp_id/src/associations/verify_updates.rs` | Client history fetch and association-state cache remain local. |
| `utils/test/tester_utils.rs::PasskeyUser`, `PKCredential`, `PKClient`, `PkUserValidationMethod` and implementations | `xmtp_id/src/utils/passkey.rs` | Client tester imports the shared signer fixture. |
| `test/mock/generate.rs::generate_inbox_id_credential` | `xmtp_id/src/utils/mod.rs` | Client mock imports it. The service duplicate must use it during handler integration. |
| `apps/mls_validation_service/src/cached_signature_verifier.rs` | `xmtp_id/src/scw_verifier/cached.rs` | Service main constructs the shared cache. Latest requests bypass it; verifier errors are not stored. |

The shared key-package constructor uses `OpenMlsProvider`. OpenMLS can store its
own bundle in that provider. The constructor does not use client database traits.
The returned `GeneratedKeyPackage` contains the bundle and optional PQ key pair.
The client still owns `store_key_package_references`, key-history writes,
serialization of database lookup keys, rotation, upload, and deletion.

The cache preserves numbered-block positive and negative verdicts, all key fields,
explicit block presence, and LRU eviction. It does not add reorganization
invalidation. This implements SEC-042 through SEC-044.

## Pending envelope integration

The Phase 1 integration owner must finish these items before closing the audit.

| Current source or missing operation | Required owner and action |
| --- | --- |
| Service `handlers.rs::validate_group_message`, `ValidateGroupMessageResult` | Move typed MLS parsing and commit/proposal classification to `xmtp_mls_validation`. Keep permissive TLS consumption and original bytes. |
| Service `validate_inbox_id_key_package` | Use `xmtp_id::key_package::VerifiedKeyPackageV2::from_bytes` through shared validation. Keep transport response construction in the service. |
| Service `get_association_state` | Reuse shared signature conversion, state folding, and state diff. Storage supplies complete history. No association-state cache in the backend. |
| Service `verify_smart_contract_wallet_signatures` | Preserve typed verifier errors. The new backend maps retryable errors to UNAVAILABLE. The old handler embeds errors in positional responses today. |
| Service `generate_inbox_id_credential`, `build_key_package_bytes` | Reuse the extracted credential generator and shared package construction in moved tests. Preserve mismatched signer/credential-key fixtures. |
| Canonical outer `Envelope` encoding and SHA-256 | Implement once in shared validation using generated protocol types. Preserve inner byte fields. |
| Structural parsing and topic derivation | Separate from cryptographic validation so duplicate lookup can precede lifetime/signature checks. An expired package can still be an exact duplicate. |
| Welcome inline/pointer parsing | Share structural version and destination extraction. Do not invoke client decryptors or registration checks. |
| `xmtp_mls/src/groups/commit_log.rs::save_remote_commit_log_entries_and_update_cursors`, inner decode | Share plaintext-entry decoding and checked group-ID extraction. Keep signature/fork checks and cursor writes on the client. |
| `groups/commit_log.rs::sign_group_logs`, per-entry encoding/signing | Split shared entry construction from private-key lookup and old publish-request construction. |
| `xmtp_db/src/encrypted_store/local_commit_log.rs::From<&LocalCommitLog> for PlaintextCommitLogEntry` | Review alongside the shared constructor. The source maps group ID, commit sequence, authenticators, result, and epoch. Keep database row/query types in `xmtp_db`; route shared protocol construction through its owning helper. |
| `xmtp_proto/src/types/topic.rs` | Add kind 0x04 and checked parsing for all kind-specific lengths. Keep topic types below validation. |

The local commit-log conversion currently casts signed database sequence/epoch
values to `u64`. Its storage adapter is not a suitable backend fixture dependency.
A shared constructor must take protocol-domain values, while database conversion
remains an adapter. The backend must not inherit client fork checks or signature
verification from that adapter (SEC-015).

## Hash boundaries

| Value | Owner and rule |
| --- | --- |
| MLS message ID | Keep `xmtp_mls/src/utils/mod.rs::id::calculate_message_id`: SHA-256 of group ID, tab, idempotency key, tab, decrypted message bytes. |
| Intent message ID | Keep `calculate_message_id_for_intent`: decodes stored intent/plaintext data and calls the MLS message-ID function. |
| MLS payload hash | Preserve `xmtp_api_d14n/src/protocol/extractors/group_messages.rs`: SHA-256 of raw `message.data`. `groups/mls_sync.rs::publish_intents` uses the matching intent payload hash. |
| Envelope hash | Shared SHA-256 of canonical protobuf envelope bytes, including framing fields. |

API-020 through API-025 and SEC-016 require these separate meanings.
`xmtp_proto::GroupMessage::is_commit` means commit only. It must not be reused as
the backend commit-or-proposal classifier.

## Complete retained inventory

All paths in this table are relative to `crates/xmtp_mls/src`. An asterisk means
all files below that directory. The extraction exceptions are listed above.

| Modules | Retained behavior and reason |
| --- | --- |
| `lib.rs`, `builder.rs`, `client.rs`, `context.rs`, `definitions.rs`, `mls_store.rs` | Client construction, facade, storage, network selection, and MLS context. `GroupCommitLock` is not the database lock required by ARC-031. |
| `cursor_store.rs`, `intents.rs`, `mutex_registry.rs`, `traits.rs` | Client cursor storage, processing errors, local locks, and conversion glue. |
| `identity.rs`, `identity_updates.rs`, `identity/*` | Keep `Identity`, `IdentityStrategy`, registration, key rotation, client state caching, DB history loading, and `StoredIdentityUpdate` glue. Extracted pure functions are listed above. |
| `registration_visible/*` | Decentralized registration visibility. No backend shared extraction; later deletion follows the project inventory. |
| `groups/mod.rs`, `groups/group_service.rs`, `groups/members.rs`, `groups/group_membership.rs`, `groups/group_permissions.rs`, `groups/validated_commit.rs` | Group state, membership, metadata, permissions, and client commit policy. Backend acceptance does not establish these properties (SEC-002, SEC-003). |
| `groups/error.rs`, `groups/summary.rs`, `groups/change_callbacks.rs`, `groups/send_message_opts.rs`, `groups/subscriptions.rs`, `groups/message_list.rs`, `groups/oneshot.rs` | Client errors, results, callbacks, content behavior, and message options. |
| `groups/intents.rs`, `groups/intents/*`, `groups/mls_sync.rs`, `groups/mls_sync/*` | Intent lifecycle, encryption, client MLS application, and local transactions. Shared construction is called through narrow adapters. |
| `groups/commit_log.rs`, `groups/commit_log_key.rs` | Keep fork state, signature policy, consensus keys, private-key storage, and re-add requests. Extract only protocol parsing/construction required above (SEC-015). |
| `groups/welcome_sync.rs`, `groups/welcomes.rs`, `groups/welcomes/*`, `groups/welcome_pointer.rs` | Client welcome join, initial membership checks, pointer resolution, and cursor writes. Backend parsing does not decrypt or verify destination membership (SEC-014). |
| `groups/mls_ext.rs`, `groups/mls_ext/*` | Keep decryptors, reload, and commit-log storage adapters. Move only the welcome capability extension listed above. |
| `groups/app_data/*` | Bootstrap validation, component source, migration, policy, sender intents, and typed facade. These require client group state; they are not backend admission checks. |
| `messages/*` | Decrypted message types, relations, reactions, and deletion checks. Backend retention is separate (SEC-004). |
| `subscriptions/*` | All catch-up, legacy/d14n conversion, processing, routing, callback, stream, watchdog, stream-stats, factory, and test modules. Phase 3 replaces old transport state. No client runtime enters the validator. |
| `worker.rs`, `worker/*` | Task lifecycle, private-key maintenance, disappearing messages, metrics, and all device-sync modules. Device-sync history storage is outside backend scope. |
| `utils/mod.rs`, `utils/cleanup_duplicate_updates.rs` | Client message IDs, HMAC epoch, version identity, and database cleanup. Crypto/time primitives already have shared owners. |
| `utils/test/*` | Keep tester macro, client builders, snapshots, proxies, delivery waits, and group helpers. Extract passkey and stateless identity material. Runtime fault flags remain client-side; shared fixtures use explicit options. |
| `test/*`, including `test/mock/*` | Keep API/context/DB mocks, stored-message generators, client builders, and sync summaries. Extract stateless credential generation. |
| `tests/*`, `groups/tests/*` | Client integration, security, migration, welcome, permissions, proposals, consent, and fork tests. Reuse shared fixtures when they replace duplicate construction. |
| `utils/bench/*` | Client/network benchmark provisioning. Not backend fixtures. |
| Crate `benches/*`, `tests/assets/*`, README/security documents and image | Client benchmarks, archive fixtures, and explanatory assets. No runtime extraction. |

## Stateless fixture requirements

ARC-012 requires valid and malformed payloads for all five kinds. Their owning
module is `xmtp_mls_validation/test-utils`; it must not depend on a full client.

- Group: application, commit, proposal, invalid TLS, non-protocol MLS body,
  wrong group-ID length, and accepted trailing bytes.
- Key package: valid signatures, invalid signatures, malformed/extraneous TLS,
  lifetime boundaries, malformed/empty credential inbox values, and installation
  key length checks during topic derivation.
- Identity: signed create/add/revoke/recovery histories, invalid signatures,
  invalid state transitions, passkey challenges, and SCW success/failure/errors.
- Welcome: inline and pointer forms, random unregistered destinations,
  invalid selection, and wrong destination length.
- Commit log: plaintext entry, retained signature, malformed entry/ID, and an
  invalid signature accepted by structural backend parsing.

`test/mock/openmls_mock.rs::BarebonesMlsClient` is not database-free: it uses
`XmtpOpenMlsProvider`, `MlsMemoryStorage`, `SqlKeyStore`, and client group
configuration. Keep those adapters in client tests. Use a direct in-memory
OpenMLS provider with the shared constructor for backend fixtures.

`GroupMessage::generate` and `test/mock/generate.rs::generate_message` produce
client wrapper fakes. Separately generated group IDs and payload hashes do not
establish a valid backend payload. The backend fixture must bind those values.

## Dependency evidence and remaining checks

- `xmtp_mls_common/Cargo.toml` has an unconditional `xmtp_db` dependency.
  Its runtime use is `group_metadata.rs::ConversationType`; migration tests also
  use it. Do not route portable validation through that dependency.
- `xmtp_id` already depends on `xmtp_proto`; `xmtp_mls_common` depends on
  `xmtp_id`. Moving the welcome capability to `xmtp_id` avoids a reverse cycle.
- `xmtp_mls/test-utils` enables DB, API stacks, archive fixtures, and native
  tooling. Never forward it into shared fixtures.
- The workspace hack enables Diesel, OpenMLS test features, and Alloy Anvil.
  Remove hack edges from the portable closure, including the host
  `xmtp_macro` path and the `xmtp_common/test-utils -> xmtp_logging` path.
- Target-gate native Anvil and Toxiproxy fixtures. Declare required OpenMLS and
  Alloy features directly. Workspace feature unification is not proof of a
  valid isolated build.
- Run isolated native and WASM validation builds/tests, with and without
  `test-utils`, then inspect normal/build dependency edges for client runtime
  and database crates.

Identity extraction checks include
`generated_package_preserves_options_and_verifies`,
`credential_shape_is_not_a_key_package_admission_rule`,
`configured_expired_lifetime_still_fails_verification`,
`cache_preserves_numbered_verdicts_and_bypasses_latest`,
`verifier_errors_are_never_cached`, `cache_key_binds_every_parameter`, and
`passkey_fixture_binds_the_signed_challenge`. The former client capability test
moved into the shared construction coverage. Final envelope test names and
native/WASM isolation results must be added by the integration owner.
