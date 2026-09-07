# Phase 1 MLS extraction audit

Status: runtime extraction and dependency isolation are implemented. The Phase 1
PR records the final native/wasm execution results. Test ownership is recorded in
[the test catalogue](tests/existing-requirements.md#phase-1-ownership-update).

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
| `test/mock/generate.rs::generate_inbox_id_credential` | `xmtp_id/src/utils/mod.rs` | Client mock imports it. The service's duplicate fixture was removed with its moved key-package tests. |
| `apps/mls_validation_service/src/cached_signature_verifier.rs` | `xmtp_id/src/scw_verifier/cached.rs` | Service main constructs the shared cache. Latest requests bypass it; verifier errors are not stored. |

The shared key-package constructor uses `OpenMlsProvider`. OpenMLS can store its
own bundle in that provider. The constructor does not use client database traits.
The returned `GeneratedKeyPackage` contains the bundle and optional PQ key pair.
The client still owns `store_key_package_references`, key-history writes,
serialization of database lookup keys, rotation, upload, and deletion.

The cache preserves numbered-block positive and negative verdicts, all key fields,
explicit block presence, and LRU eviction. It does not add reorganization
invalidation. This implements SEC-042 through SEC-044.

## Shared envelope integration

The original sources now use these shared owners or remain transport/storage adapters.

| Original source or operation | Current owner and boundary |
| --- | --- |
| Service `handlers.rs::validate_group_message`, `ValidateGroupMessageResult` | `xmtp_mls_validation::{parse_group_message,is_commit_or_proposal}`. The old service converts the shared result to its response shape. Trailing bytes remain accepted. |
| Service `validate_inbox_id_key_package` | `xmtp_mls_validation::verify_key_package` calls the existing identity verifier. The service retains response construction. |
| Service `get_association_state` | `xmtp_mls_validation::validate_identity_updates` uses shared signature conversion, state folding, and state diff. The caller supplies complete history. |
| Service `verify_smart_contract_wallet_signatures` | The shared `xmtp_id` verifier and cache return typed errors. The old RPC adapter retains positional response construction. Phase 2 maps retryable errors for the new API. |
| Service key-package fixtures | `xmtp_mls_validation::test_utils` uses shared package construction and retains mismatched signer fixtures. The old duplicate builders were removed. |
| Canonical outer `ClientEnvelope` encoding and SHA-256 | `xmtp_proto::types::canonical_envelope` encodes once and preserves all inner byte fields. |
| Structural parsing and topic derivation | `xmtp_mls_validation::parse_envelope` precedes `validate_envelope`. An expired key package still yields a topic and canonical hash for duplicate lookup. |
| Welcome inline/pointer parsing | `parse_envelope` checks the selected version and destination length without decryption or registration checks. |
| Client commit-log inner decode | `xmtp_mls_common::commit_log::decode_commit_log` is shared. The backend parser checks the embedded group ID; the client retains its existing signature/fork checks, check order, and cursor writes. |
| `groups/commit_log.rs::sign_group_logs`, per-entry encoding/signing | `xmtp_mls_common::commit_log::sign_commit_log` owns encoding and signing. Private-key lookup and old publish-request construction remain in the client. |
| `xmtp_db/src/encrypted_store/local_commit_log.rs::From<&LocalCommitLog> for PlaintextCommitLogEntry` | Kept as a client storage adapter. It maps stored fields to protocol fields; the backend has no client `LocalCommitLog` rows. Shared signing consumes the resulting protocol entry. |
| `xmtp_proto/src/types/topic.rs` | Kind 0x04 and `Topic::parse` implement checked backend topics. Existing client constructors retain their behavior. |

The local commit-log conversion currently casts signed database sequence/epoch
values to `u64`. Its storage adapter is not a suitable backend fixture dependency.
The shared signer takes protocol-domain values; database conversion remains an
adapter. Backend admission does not inherit client fork checks or signature
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

## Dependency evidence

The PR #4067 ownership review also moved general commit-log signing and decoding
to `xmtp_mls_common::commit_log`. The remaining validation utilities parse
admission payloads, classify commits/proposals, check key packages, or validate
identity histories. Its `test-utils` generators remain admission fixtures.
Canonical outer-envelope encoding and hashing remain in `xmtp_proto`.

- `xmtp_mls_common` has no `xmtp_db` dependency. The shared
  `xmtp_proto::types::ConversationType` keeps metadata and migration code portable.
  Its SQL conversions require the existing `xmtp_proto/diesel` feature.
- `xmtp_id` already depends on `xmtp_proto`; `xmtp_mls_common` depends on
  `xmtp_id`. Moving the welcome capability to `xmtp_id` avoids a reverse cycle.
- `xmtp_mls/test-utils` enables DB, API stacks, archive fixtures, and native
  tooling. Never forward it into shared fixtures.
- Hakari final exclusions remove the aggregate feature helper from the portable
  closure, including `xmtp_macro` and the test-only `xmtp_logging` path.
- Portable `test-utils` is separate from native `test-utils-network` and
  `test-utils-anvil` harness features. Required OpenMLS and Alloy features are
  declared directly.
- `dev/check-validation` compiles standalone consumers and checks normal/build
  graphs on native and wasm, with and without fixtures. It rejects client
  runtimes, databases, the workspace helper, Anvil, and Toxiproxy.

Identity extraction checks include
`generated_package_preserves_options_and_verifies`,
`credential_shape_is_not_a_key_package_admission_rule`,
`configured_expired_lifetime_still_fails_verification`,
`cache_preserves_numbered_verdicts_and_bypasses_latest`,
`verifier_errors_are_never_cached`, `cache_key_binds_every_parameter`, and
`passkey_fixture_binds_the_signed_challenge`. The former client capability test
moved into the shared construction coverage. The test catalogue records the
envelope matrices, existing requirement IDs, and retired placeholder tests.
