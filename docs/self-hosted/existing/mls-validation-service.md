<!-- markdownlint-configure-file { "MD024": { "siblings_only": true }, "MD029": false } -->

# MLS Validation Service — Existing Behavior

Reference page for Phase 0. It records what the standalone `mls_validation_service`
validates today, its inputs and outputs, its error cases, and how the Go backends use
it. The new self-hosted Rust backend must offer the same behavior from a crate.

All paths are absolute. Every claim cites `path:symbol`.

Repos referenced:

- `/Users/nickmolnar/code/xmtp/libxmtp` — the service and all validation logic.
- `/Users/nickmolnar/code/xmtp/proto` — the gRPC contract.
- `/Users/nickmolnar/code/xmtp/xmtp-node-go` — v3 backend caller.
- `/Users/nickmolnar/code/xmtp/xmtpd` — d14n backend caller.

---

## Table of contents

1. [Service overview](#1-service-overview)
2. [RPCs](#2-rpcs)
   - [2.1 ValidateInboxIdKeyPackages](#21-validateinboxidkeypackages)
   - [2.2 ValidateGroupMessages](#22-validategroupmessages)
   - [2.3 GetAssociationState](#23-getassociationstate)
   - [2.4 VerifySmartContractWalletSignatures](#24-verifysmartcontractwalletsignatures)
   - [2.5 ValidateKeyPackages (legacy)](#25-validatekeypackages-legacy)
   - [2.6 Commit-log validation](#26-commit-log-validation)
3. [Identity association validation in depth](#3-identity-association-validation-in-depth)
4. [Smart contract wallet verification](#4-smart-contract-wallet-verification)
5. [Backend wiring](#5-backend-wiring)
6. [Configuration, health check, packaging](#6-configuration-health-check-packaging)
7. [Tests](#7-tests)
8. [Summary for the crate implementer](#8-summary-for-the-crate-implementer)
9. [Review status](#review-status)

---

## 1. Service overview

The service is a small tonic gRPC server. It has no database and no state except an
LRU cache of smart contract signature results.

Entry point: `/Users/nickmolnar/code/xmtp/libxmtp/apps/mls_validation_service/src/main.rs:main`.

Startup order in `main`:

1. Parse CLI args (`config::Args`).
2. Set up tracing. `LogFormat::Json` uses a JSON layer; `LogFormat::Text` uses ANSI
   only when stdout is a terminal.
3. Log the version (`version.rs:get_version`, `CARGO_PKG_VERSION` + `VERGEN_GIT_SHA`).
   If `--version` was passed, exit.
4. Start the HTTP health check server on `health_check_port`.
5. Build the chain verifier: `MultiSmartContractSignatureVerifier::new_from_file` when
   `--chain-urls <path>` is given, else `MultiSmartContractSignatureVerifier::new_from_env`.
6. Wrap it in `CachedSmartContractSignatureVerifier` with `--cache-size` entries.
7. Serve `ValidationApiServer::new(ValidationService::new(cached_verifier))` on
   `0.0.0.0:<port>`, with graceful shutdown on SIGINT or SIGTERM
   (`main.rs:wait_for_quit`).

The service struct holds only the verifier:
`apps/mls_validation_service/src/handlers.rs:ValidationService` has one field,
`scw_verifier: Box<dyn SmartContractSignatureVerifier>`.

### Error → gRPC status mapping

`handlers.rs:GrpcServerError` wraps four error kinds: `Deserialization`, `Association`,
`Signature`, `Conversion`.

`handlers.rs:impl From<GrpcServerError> for Status` decides the code:

| Condition | gRPC code |
| --- | --- |
| `err.is_retryable() == true` | `Code::Unavailable` |
| otherwise | `Code::InvalidArgument` |

`handlers.rs:impl RetryableError for GrpcServerError` defines retryable as:

- `Signature(e)` → `e.is_retryable()`
- `Association(AssociationError::Signature(e))` → `e.is_retryable()`
- `Conversion(e)` → `e.is_retryable()`, which is **always `false`**:
  `crates/xmtp_proto/src/error.rs:impl RetryableError for ConversionError` returns `false`
  unconditionally. So `Conversion` always maps to `InvalidArgument`.
- `Deserialization(_)` and every other `Association(_)` → `false`

**Which variant do identity-update protos produce?** `Conversion`, not `Deserialization`.
`try_map_vec` in `get_association_state` uses
`crates/xmtp_id/src/associations/serialization.rs:impl TryFrom<IdentityUpdateProto> for UnverifiedIdentityUpdate`,
whose `type Error = ConversionError`, and the `?` converts it through
`GrpcServerError::Conversion(#[from] xmtp_proto::ConversionError)`.
`GrpcServerError::Deserialization` is reached only from the
`VerifySmartContractWalletSignatures` handler, for `InvalidAccountId` and `InvalidHash`
(`handlers.rs`). This matters downstream: the `error-code` label is a
`ConversionError::*` label, and xmtpd's substring list looks for `"Conversion error"`,
which no `ConversionError` Display string contains — see section 5.2.

The status also carries a metadata header `error-code` with the stable label from
`xmtp_common::ErrorCode` (for example `"SignatureError::VerifierError"`). The trait is
`/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_common/src/error_code.rs:ErrorCode`;
codes are formatted `"TypeName::VariantName"`.

**Important:** this mapping applies only to `GetAssociationState`. The other three RPC
handlers never return `Err`; they report per-item failures inside the response body.

---

## 2. RPCs

The contract is
`/Users/nickmolnar/code/xmtp/proto/proto/mls_validation/v1/service.proto`, service
`xmtp.mls_validation.v1.ValidationApi`. It declares exactly four RPCs. The generated
Rust server trait confirms this:
`/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_proto/src/gen/xmtp.mls_validation.v1.rs`
declares `validate_group_messages`, `get_association_state`,
`validate_inbox_id_key_packages`, `verify_smart_contract_wallet_signatures`.

### 2.1 ValidateInboxIdKeyPackages

Handler: `handlers.rs:ValidationApi::validate_inbox_id_key_packages`.
Worker: `handlers.rs:validate_inbox_id_key_package`.

#### Request

`ValidateKeyPackagesRequest` (note: the request type is the *legacy* message name,
reused by this RPC).

| Field | Type | Use |
| --- | --- | --- |
| `key_packages[].key_package_bytes_tls_serialized` | `bytes` | The only field read. |
| `key_packages[].is_inbox_id_credential` | `bool` | **Ignored.** The handler maps each entry to its bytes only and drops this flag. |

#### Steps

Per key package, in order:

1. Create a fresh `openmls_rust_crypto::RustCrypto` provider (per call, in
   `validate_inbox_id_key_package`).
2. `VerifiedKeyPackageV2::from_bytes(&rust_crypto, bytes)` —
   `/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_id/src/key_package/verified_key_package_v2.rs:VerifiedKeyPackageV2::from_bytes`:
   1. `KeyPackageIn::tls_deserialize_exact(data)` — strict TLS decode; trailing bytes
      are an error.
   2. `kp_in.validate(crypto_provider, MLS_PROTOCOL_VERSION, LeafNodeLifetimePolicy::Verify)`.
      `MLS_PROTOCOL_VERSION` is `ProtocolVersion::Mls10`
      (`crates/xmtp_configuration/src/common/mls.rs:MLS_PROTOCOL_VERSION`). OpenMLS's
      `validate` (`openmls/src/key_packages/key_package_in.rs:KeyPackageIn::validate`,
      the `xmtp/openmls` fork pinned in `Cargo.toml`) performs these checks **in this
      exact order**:

      1. Derive the signature algorithm from the **key package's own declared
         ciphersuite** (`self.payload.ciphersuite.signature_algorithm()`).
      2. **Leaf node source is `KeyPackage`** and **the leaf node signature is verified**
         against the leaf node's own signature key. These are fused in one `match`:
         a non-`KeyPackage` source gives `InvalidLeafNodeSourceType`, a bad signature
         gives `InvalidLeafNodeSignature`.
      3. **Protocol version** match (exact equality with `Mls10`) →
         `InvalidProtocolVersion`.
      4. **Init key ≠ encryption key** → `InitKeyEqualsEncryptionKey`.
      5. Convert the extensions to key-package extension types.
      6. **The outer KeyPackage signature is verified** over the `KeyPackageTbs` →
         `InvalidSignature`.
      7. For each **key-package-level** extension, the leaf node must support it
         (`leaf_node.supports_extension`) → `UnsupportedExtension`.
      8. Because the policy is `Verify`: the leaf node must carry a lifetime
         (`MissingLifetime`) and `not_before <= now < not_after` must hold
         (`LifetimeError`).

      Note the ordering consequence: the leaf node signature is checked first, and the
      **outer KeyPackage signature is checked only at step 6**. Steps 3 and 4 therefore
      run on data that the key package signature has not yet authenticated.

      Three limits of this path matter for the new crate:

      - **No ciphersuite validation of any kind.** There is no key-package-vs-leaf-node
        ciphersuite comparison — a leaf node carries no ciphersuite field at all — and
        there is **no XMTP ciphersuite allow-list** on this path. libxmtp's `CIPHERSUITE`
        and `POST_QUANTUM_CIPHERSUITE` constants are used only by
        `crates/xmtp_id/src/key_package/mls_ext_wrapper_encryption.rs`, never here. A key
        package declaring an arbitrary ciphersuite passes as long as its signature
        algorithm verifies.
      - **The leaf-node-local checks do not run.** `LeafNode::validate_locally`
        (`openmls/src/treesync/node/leaf_node.rs`) is not called on this path, so leaf
        extension type validity, **leaf extensions covered by leaf capabilities**, and
        **credential type covered by leaf capabilities** are all unchecked. Only
        key-package-level extensions are compared against leaf capabilities (step 7).
      - **The lifetime maximum-range check does not run.** `Lifetime::has_acceptable_range`
        (`openmls/src/key_packages/lifetime.rs`) has no caller anywhere in either repo, so
        an excessively long lifetime — a 100-year window — is accepted.
   3. `TryFrom<KeyPackage> for VerifiedKeyPackageV2` — take `kp.leaf_node()`, convert the
      credential to `BasicCredential` (fails if it is not a basic credential), read
      `leaf_node.signature_key()` as `installation_public_key`, and prost-decode the
      credential identity bytes into `xmtp_proto::xmtp::identity::MlsCredential` (which
      carries `inbox_id`).

      **There is no `inbox_id` format validation.** The decoded value is moved straight
      into the result with no inspection. `MlsCredential` is a single `string inbox_id`
      field (`/Users/nickmolnar/code/xmtp/proto/proto/identity/credential.proto`), so the
      requirement is only "Basic credential whose identity bytes prost-decode as
      `MlsCredential`". The `inbox_id` may be empty, non-hex, the wrong length, or
      arbitrarily long. Nothing here ties it to the installation key either — see Notes.
3. On success build a response with `is_ok = true`, the decoded `credential`, and the
   `installation_public_key`.

Every key package is submitted to `futures::future::join_all`, so one bad key package
never fails the others and results stay positional. This is **failure isolation, not
parallelism**: the worker `validate_inbox_id_key_package` contains no `.await` point, so
the CPU validation runs sequentially on the task thread when the futures are polled
(`handlers.rs:validate_inbox_id_key_packages`, `handlers.rs:validate_inbox_id_key_package`).

#### Response

`ValidateInboxIdKeyPackagesResponse.responses[]`:

| Field | Value on success | Value on failure |
| --- | --- | --- |
| `is_ok` | `true` | `false` |
| `error_message` | `""` | see errors table |
| `credential` | decoded `MlsCredential` (contains `inbox_id`) | `None` |
| `installation_public_key` | leaf node signature key bytes | `[]` |
| `expiration` | **always `0`** | `0` |

The `expiration` field is hard-coded to `0` with the comment "We are deprecating the
expiration field and key package lifetimes, so stop checking for its existence"
(`handlers.rs:validate_inbox_id_key_package`). The key package *lifetime window* is still
verified inside OpenMLS via `LeafNodeLifetimePolicy::Verify` — though only as
`not_before <= now < not_after`, with no maximum-range bound — but the value is not
returned. `VerifiedKeyPackageV2::life_time` exists and catches a panic from
`inner.life_time()`, but it is a plain accessor that validates nothing, and the handler
does not call it.

#### Errors

The handler never returns a gRPC error. Failures become `is_ok = false` with a message
built by `handlers.rs:impl From<ValidateInboxIdKeyPackageError> for ValidateInboxIdKeyPackageResponse`.
The wrapper format is `"XMTP Key Package failed {0}"`
(`handlers.rs:ValidateInboxIdKeyPackageError::KeyPackageVerification`), where `{0}` is a
`KeyPackageVerificationError` from
`crates/xmtp_id/src/key_package/verified_key_package_v2.rs:KeyPackageVerificationError`:

| Inner variant | Inner string | Full message shape |
| --- | --- | --- |
| `TlsError(TlsCodecError)` | `"TLS Codec error: {0}"` | `XMTP Key Package failed TLS Codec error: …` |
| `MlsValidation(KeyPackageVerifyError)` | `"mls validation: {0}"` | `XMTP Key Package failed mls validation: The leaf node signature is not valid.` |
| `WrongCredentialType(BasicCredentialError)` | `"wrong credential type"` | `XMTP Key Package failed wrong credential type` |
| `ConversionError(xmtp_proto::ConversionError)` | transparent | varies |

`prost::DecodeError` converts into `ConversionError`
(`verified_key_package_v2.rs:impl From<prost::DecodeError> for KeyPackageVerificationError`).

The exact string `"XMTP Key Package failed mls validation: The leaf node signature is not valid."`
is asserted in `handlers.rs:tests::test_validate_inbox_id_key_package_failure`.

#### Notes

- The RPC deliberately does **not** check that the `inbox_id` in the credential really
  owns the installation key. The proto comment says so: "without checking whether an
  InboxId <> InstallationPublicKey pair is really valid." That association check is the
  caller's job, using `GetAssociationState`. Combined with the absence of any `inbox_id`
  format validation, the guarantee this RPC offers is narrow: the key package is
  well-formed and self-consistent, and it carries *some* string in a Basic credential.
- Wrapper-encryption extension (`WELCOME_WRAPPER_ENCRYPTION_EXTENSION_ID = 0xff03`,
  `crates/xmtp_configuration/src/common/metadata.rs`) is parsed by
  `VerifiedKeyPackageV2::wrapper_encryption` but the handler does not call it and does
  not return it.

### 2.2 ValidateGroupMessages

Handler: `handlers.rs:ValidationApi::validate_group_messages`.
Worker: `handlers.rs:validate_group_message` (synchronous, not async).

#### Request

`ValidateGroupMessagesRequest.group_messages[].group_message_bytes_tls_serialized` —
one TLS-serialized `MlsMessageIn` per entry.

#### Steps

1. `MlsMessageIn::tls_deserialize(&mut message.as_slice())`. Note this is
   `tls_deserialize`, not `tls_deserialize_exact`, so trailing bytes are not rejected.
2. `msg_result.try_into_protocol_message()` — fails for message types that are not a
   `ProtocolMessage` (for example a `KeyPackage` or `Welcome` payload).
3. `group_id = hex::encode(protocol_message.group_id().as_slice())`.
4. `is_commit = matches!(protocol_message.content_type(), ContentType::Commit | ContentType::Proposal)`.

**This is the whole validation.** There is no signature check, no epoch check, no
membership check, no ciphersuite check, and no decryption. The service only parses the
MLS framing and reports the group id and whether the content type is a commit *or a
proposal*.

#### What `group_id` does and does not prove

This RPC **parses only**. Read the consequence plainly:

- The group id is read out of the client-supplied ciphertext. The client controls those
  bytes, so the client chooses the group id it lands on.
- The service **cannot verify that the sender is a member of that group**. It has no
  group state, no roster, and no key material, so it cannot check the sender's signature
  or membership.
- Therefore **any client can publish a message to any group id** it names. Parsing binds
  storage routing to the group id inside the message; it does not authorize the sender.

This is **a property, not a bug**. It follows from MLS confidentiality: the group's
membership and the sender's identity are inside the encrypted payload, and the server
holds no group secrets, so a server-side membership check is not possible by design.
Real access control is the group's own MLS state on member devices — a message from a
non-member fails authentication when members process it. The server-side effect is
limited to spam and storage noise on a topic, not to reading or forging group content.

The new crate must not describe `group_id` parsing as an authorization check. It is a
routing key derived from the ciphertext rather than from a separate client field, which
stops a client from *mislabeling* a message it did author, and nothing more.

#### Response

`ValidateGroupMessagesResponse.responses[]`:

| Field | Success | Failure |
| --- | --- | --- |
| `is_ok` | `true` | `false` |
| `error_message` | `""` | the `Display` string of the TLS or conversion error |
| `group_id` | hex of the MLS group id | `""` |
| `is_commit` | `true` for Commit **or Proposal** content type | `false` |

There are **no** `epoch` or `sender` fields. The generated Rust proto
(`crates/xmtp_proto/src/gen/xmtp.mls_validation.v1.rs`, module
`validate_group_messages_response`) has exactly four fields: `is_ok` (tag 1),
`error_message` (tag 2), `group_id` (tag 3), `is_commit` (tag 4). The `.proto` agrees.
So no backend can derive epoch or sender ordering from this RPC.

#### Errors

Never a gRPC error. Errors are per-message strings from
`tls_codec::Error::to_string()` or from `try_into_protocol_message`'s error.

#### Notes

- `is_commit` being true for proposals is a naming quirk worth preserving or fixing
  deliberately; changing it changes backend behavior.
- Because the RPC does no cryptographic work, moving it into the new backend is a pure
  parsing concern with an `openmls` dependency only.

### 2.3 GetAssociationState

Handler: `handlers.rs:ValidationApi::get_association_state`.
Worker: `handlers.rs:get_association_state`.

This is the only RPC that can return a gRPC error, and the only one that can make
network calls (to a chain RPC, for smart contract wallet signatures).

#### Request

`GetAssociationStateRequest`:

| Field | Type | Meaning |
| --- | --- | --- |
| `old_updates` | `repeated xmtp.identity.associations.IdentityUpdate` | Already-accepted updates for the inbox, in log order. |
| `new_updates` | `repeated xmtp.identity.associations.IdentityUpdate` | Updates being proposed now. |

#### Steps

In `handlers.rs:get_association_state`:

1. `try_map_vec(old_updates)` and `try_map_vec(new_updates)` convert protos into
   `UnverifiedIdentityUpdate`
   (`crates/xmtp_id/src/associations/serialization.rs:try_map_vec`). Failure →
   **`GrpcServerError::Conversion`**, not `Deserialization`. The conversion is
   `serialization.rs:impl TryFrom<IdentityUpdateProto> for UnverifiedIdentityUpdate`,
   whose `type Error = ConversionError`. Always non-retryable → `InvalidArgument`.
2. `try_join_all(old.iter().map(|u| u.to_verified(&scw_verifier)))` — verify every
   signature in every old update. Same for `new_updates`. Failure →
   `GrpcServerError::Signature`.
3. Branch:
   - **`old_updates` empty:** `new_state = associations::get_state(&new_updates)`; return
     `association_state = new_state`, `state_diff = new_state.as_diff()`. `as_diff`
     reports every member as new and nothing as removed
     (`crates/xmtp_id/src/associations/state.rs:AssociationState::as_diff`).
   - **otherwise:** `old_state = associations::get_state(&old_updates)`, then fold each
     new update with `associations::apply_update(state, update)`, then
     `state_diff = old_state.diff(&new_state)`.
4. Convert both to protos and return.

Note the asymmetry: when `old_updates` is empty the diff is "everything is new"; when it
is non-empty the diff is the true set difference between old and new member sets.

`get_state` and `apply_update` are in
`crates/xmtp_id/src/associations/mod.rs:get_state` and
`crates/xmtp_id/src/associations/mod.rs:apply_update`. Section 3 documents the rules
they enforce.

#### Response

`GetAssociationStateResponse`:

| Field | Meaning |
| --- | --- |
| `association_state` | Final `AssociationState`: `inbox_id`, `members[]`, `recovery_identifier`, `seen_signatures`. |
| `state_diff` | `AssociationStateDiff` with `new_members[]` and `removed_members[]`. |

`AssociationStateDiff` has helpers `new_installations()` and `removed_installations()`
that filter to installation-kind members
(`crates/xmtp_id/src/associations/state.rs:AssociationStateDiff`).

#### Errors

| Cause | `GrpcServerError` variant | gRPC code | Notes |
| --- | --- | --- | --- |
| Proto → unverified conversion failed | `Conversion(xmtp_proto::ConversionError)` | `InvalidArgument` | Never retryable. **Not** `Deserialization` — see step 1. |
| Signature verification failed (bad ECDSA, bad ed25519, bad passkey, SCW returned invalid) | `Signature(SignatureError)` | `InvalidArgument` for terminal variants | `SignatureError::Invalid` etc. |
| Chain RPC unreachable / no verifier for chain | `Signature(SignatureError::VerifierError(..))` | `Unavailable` | Retryable; see section 4. |
| Association rule violated (replay, wrong inbox id, missing member, …) | `Association(AssociationError)` | `InvalidArgument` | Not retryable, except `Association(AssociationError::Signature(e))` where `e` is retryable. |
| Conversion error | `Conversion(xmtp_proto::ConversionError)` | depends on `is_retryable` | |

`DeserializationError` has 15 variants
(`crates/xmtp_id/src/associations/serialization.rs:DeserializationError`): a wrapped
`SignatureError` (transparent), `MissingAction` ("Missing action"), `MissingUpdate`
("Missing update"), `MissingMemberIdentifier` ("Missing member identifier"), `Signature`
("Missing signature"), `MissingMember` ("Missing Member"), `Decode` ("Decode error {0}"),
`InvalidAccountId` ("Invalid account id"), `InvalidPasskey` ("Invalid passkey"),
`InvalidHash` ("Invalid hash (needs to be 32 bytes)"), `Unspecified(&'static str)`
("A required field is unspecified: {0}"), `Deprecated(&'static str)` ("Field is
deprecated: {0}"), **`Ed25519`** ("Error creating public key from proto bytes"),
**`Bincode`** ("Unable to deserialize"), and **`AddressValidation`** (transparent, wraps
`IdentifierValidationError`).

Remember from step 1 that on the `GetAssociationState` path this enum is **not** reached.
It is used only by the `VerifySmartContractWalletSignatures` handler, for
`InvalidAccountId` and `InvalidHash`.

`AssociationError` variants and strings are in
`crates/xmtp_id/src/associations/association_log.rs:AssociationError`; see the table in
section 3.

#### Notes

- The whole call is all-or-nothing. One bad update fails the entire request. Callers get
  no partial state.
- The verification step is concurrent per update and per action, so a batch with many
  smart contract wallet signatures issues many chain calls at once.

### 2.4 VerifySmartContractWalletSignatures

Handler: `handlers.rs:ValidationApi::verify_smart_contract_wallet_signatures`.
Worker: `handlers.rs:verify_smart_contract_wallet_signatures`.

The proto reuses the node's identity API messages
(`xmtp.identity.api.v1.VerifySmartContractWalletSignatures{Request,Response}`). The
proto comment says "This request is proxied from the node, so we'll reuse those
messages."

#### Request

`signatures[]` of `VerifySmartContractWalletSignatureRequestSignature`:

| Field | Type | Use |
| --- | --- | --- |
| `account_id` | `string` | A CAIP-10 **string**, not a message (`/Users/nickmolnar/code/xmtp/proto/proto/identity/api/v1/identity.proto`). Converted to `xmtp_id::associations::AccountId`. |
| `hash` | `bytes` | Must be exactly 32 bytes. |
| `signature` | `bytes` | Passed through to the verifier. |
| `block_number` | `optional uint64` | `None` means "use the current head block". |

#### Steps

Per signature, concurrently via `join_all`:

1. `signature.account_id.try_into()` → `AccountId`. On failure →
   `DeserializationError::InvalidAccountId`.
2. `signature.hash.try_into()` → `[u8; 32]`. On failure → `DeserializationError::InvalidHash`.
3. `scw_verifier.is_valid_signature(account_id, hash, signature.into(), block_number)`.
   On error → `SignatureError::VerifierError(e)`.

#### Response

`VerifySmartContractWalletSignaturesResponse.responses[]`:

| Field | Success | Failure |
| --- | --- | --- |
| `is_valid` | from `ValidationResponse::is_valid` | `false` |
| `block_number` | from `ValidationResponse::block_number` (always `Some` in practice) | `None` |
| `error` | `None` | `Some(format!("{err:?}"))` — the **Debug** formatting of `GrpcServerError` |

#### Errors

The handler never returns a gRPC error. Failures are per-signature and stringified with
`{:?}` (Debug), not `{}` (Display), so the returned text is the Rust debug
representation of the error enum. Worth noting if the new crate wants stable error text.

#### Notes

- This RPC is why the service needs chain RPC access and outbound network egress.
- It is proxied: clients call the *node's* identity API, and the node forwards here. See
  section 4.

### 2.5 ValidateKeyPackages (legacy)

There is **no `ValidateKeyPackages` RPC**. The proto declares the *messages*
`ValidateKeyPackagesRequest` and `ValidateKeyPackagesResponse`
(`/Users/nickmolnar/code/xmtp/proto/proto/mls_validation/v1/service.proto`), and
`ValidateKeyPackagesRequest` is still the request type of
`ValidateInboxIdKeyPackages`, but no `rpc ValidateKeyPackages(...)` exists. The
generated Rust `ValidationApi` trait
(`crates/xmtp_proto/src/gen/xmtp.mls_validation.v1.rs`) has only the four methods listed
above.

`ValidateKeyPackagesResponse.ValidationResponse` (fields `is_ok`, `error_message`,
`installation_id`, `account_address`, `credential_identity_bytes`, `expiration`) is dead
message-only surface. The new crate does not need it.

Note the naming trap for the implementer: xmtpd's Go method is named
`ValidateKeyPackages` but it calls the `ValidateInboxIdKeyPackages` RPC
(`/Users/nickmolnar/code/xmtp/xmtpd/pkg/mlsvalidate/service.go:MLSValidationServiceImpl.ValidateKeyPackages`).

### 2.6 Commit-log validation

**None exists.** There is no commit-log RPC in the proto, and no commit-log code in
`apps/mls_validation_service/src/`. A search for `commit_log` / `CommitLog` in the
validation proto directory and the service source returns nothing. If the new backend
needs commit-log validation it is new work, not a port.

---

## 3. Identity association validation in depth

All of this lives in
`/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_id/src/associations/`.

### 3.1 Two-phase model

Validation is split deliberately:

1. **Signature verification** (`unverified.rs`): turn `UnverifiedIdentityUpdate` into
   `IdentityUpdate` by verifying every signature. This is the phase that can touch the
   network.
2. **State machine** (`association_log.rs`): apply verified updates in order to build an
   `AssociationState`. Pure, no I/O.

The service runs both, in that order, in `handlers.rs:get_association_state`.

### 3.2 The signature text

Every signature in an update signs the *same* text, computed once per update from the
unsigned actions: `unverified.rs:UnverifiedIdentityUpdate::signature_text` calls
`unsigned_actions.rs:UnsignedIdentityUpdate::signature_text`, which produces:

```text
XMTP : Authenticate to inbox

Inbox ID: <inbox_id>
Current time: <RFC3339 seconds, UTC, from client_timestamp_ns>

- <action line 1>
- <action line 2>

For more info: https://xmtp.org/signatures
```

Action lines (`unsigned_actions.rs`):

| Action | Line |
| --- | --- |
| `UnsignedCreateInbox` | `- Create inbox\n  (Owner: <identifier>)` |
| `UnsignedAddAssociation`, installation | `- Grant messaging access to app\n  (ID: <hex key>)` |
| `UnsignedAddAssociation`, ethereum | `- Link address to inbox\n  (Address: <addr>)` |
| `UnsignedAddAssociation`, passkey | `- Link passkey to inbox\n  (Passkey: <hex key>)` |
| `UnsignedRevokeAssociation`, installation | `- Revoke messaging access from app\n  (ID: <hex key>)` |
| `UnsignedRevokeAssociation`, ethereum | `- Unlink address from inbox\n  (Address: <addr>)` |
| `UnsignedRevokeAssociation`, passkey | `- Unlink passkey from inbox\n  (Passkey: <hex key>)` |
| `UnsignedChangeRecoveryAddress` | `- Change inbox recovery address\n  (Address: <identifier>)` |

**Revoke carries a second target line.** Earlier drafts of this page listed the revoke
prefix alone. That was wrong: `unsigned_actions.rs:impl SignatureTextCreator for UnsignedRevokeAssociation`
appends `\n  ({id_kind}: {})` with the revoked member, exactly like the add actions. This
matters because `RevokeAssociation::update_state` never re-checks `revoked_member` against
anything else — **this line is the only binding of the revocation target to the
signature**. Drop it and any revoke signature would authorize revoking any member.

Note `UnsignedChangeRecoveryAddress` hard-codes the label `Address:` even when the new
recovery identifier is a passkey; it does not use `get_identifier_text`.

The header and footer constants are `unsigned_actions.rs:HEADER` /
`unsigned_actions.rs:FOOTER`:

- `HEADER` = `XMTP : Authenticate to inbox`
- `FOOTER` = `For more info: https://xmtp.org/signatures` — **no trailing slash.**

The legacy "Create Identity" transcript is a different format with a **trailing slash**;
see section 3.4.

`unsigned_actions.rs:pretty_timestamp` renders `client_timestamp_ns` as
`DateTime::from_timestamp_nanos(ns as i64).to_rfc3339_opts(SecondsFormat::Secs, true)`.
The exact expected text is pinned by
`unsigned_actions.rs:tests::create_signatures`.

**Consequence for `client_timestamp_ns`:** it is part of the signed payload, so it
cannot be altered after signing without invalidating every signature. But the backend
does **not** check it against wall-clock time, does not bound its skew, and does not
require it to increase across updates. Its only other role is being stored on new
members as `Member::client_timestamp_ns` (`association_log.rs:AddAssociation::update_state`
passes `Some(client_timestamp_ns)` into `Member::new`), which is used only for sorting
(`state.rs:AssociationState::members` sorts by `client_timestamp_ns`, missing sorts
last). Ordering of updates comes from the caller's array order, not from the timestamp.

Also note `association_log.rs:IdentityUpdate::update_state` passes `self.client_timestamp_ns`
to each action, ignoring the `_client_timestamp_ns` argument it was called with.

### 3.3 inbox_id derivation

`crates/xmtp_id/src/associations/member.rs:Identifier::inbox_id`:

```rust
pub fn inbox_id(&self, nonce: u64) -> Result<String, AssociationError> {
    if !self.is_valid_address() {
        return Err(AssociationError::InvalidAccountAddress);
    }
    let ident: MemberIdentifier = self.clone().into();
    Ok(sha256_string(format!("{ident}{nonce}")))
}
```

So `inbox_id = hex(sha256(display(identifier) || decimal(nonce)))`.

- For Ethereum, `Display` is the address string as stored, verbatim
  (`crates/xmtp_id/src/associations/ident/ethereum.rs:impl Display for Ethereum`).

  **Canonicalization is lowercasing, not EIP-55.**
  `crates/xmtp_cryptography/src/signature.rs:sanitize_evm_addresses` validates each
  address and returns `addr.to_lowercase()`. No EIP-55 checksum is ever produced or
  verified anywhere in the repo. `crates/xmtp_cryptography/src/signature.rs:h160addr_to_string`
  likewise emits `0x` + lowercase hex, so every **recovered signer** is canonical
  lowercase.

  **And the service decode path does not canonicalize at all.** Sanitizing happens only
  in the `Identifier::eth` / `MemberIdentifier::eth` constructors
  (`member.rs:Identifier::eth` → `Ethereum::sanitize` → `sanitize_evm_addresses`). The
  proto conversions the service actually uses —
  `crates/xmtp_id/src/associations/member.rs:Identifier::from_proto` and
  `crates/xmtp_id/src/associations/serialization.rs:impl TryFrom<MemberIdentifierProto> for MemberIdentifier`
  — build `ident::Ethereum(address)` straight from the supplied string. They neither
  sanitize nor validate. **The original text is preserved exactly as the client sent it.**

  Two consequences follow, and they pull in opposite directions:

  - `inbox_id` hashes the **verbatim stored string** plus the decimal nonce, so
    differently-cased spellings of one address would derive different inbox ids.
  - `Ethereum` derives `PartialEq`/`Hash` on the raw `String`, so comparison is
    **case-sensitive**. Because recovered signers are always canonical lowercase, any
    check of a proto-supplied identifier against a recovered signer **fails closed** on
    non-canonical input — `CreateInbox` gives `MissingExistingMember` and `AddAssociation`
    gives `NewMemberIdSignatureMismatch`. So mixed case cannot mint an alternate inbox id
    for a key you control.

  The gap is `ChangeRecoveryIdentity::update_state`, which writes
  `new_recovery_identifier` into state with no `is_valid_address` check and no
  sanitizing. A malformed or mixed-case recovery identifier can be stored, after which no
  signer can ever match it. The new crate should canonicalize on the decode path.
- For Passkey, `Display` is `hex::encode(key)`
  (`crates/xmtp_id/src/associations/ident/passkey.rs:impl Display for Passkey`).
- `member.rs:Identifier::is_valid_address` requires an Ethereum address to be exactly 42
  chars, start with `0x`, and be all hex after that; non-Ethereum identifiers always
  pass. Failure is `AssociationError::InvalidAccountAddress` with the string
  `"Invalid account address: Must be 42 hex characters, starting with '0x'."`

The derived id is enforced twice: `AssociationState::new` derives it from the create
action (`state.rs:AssociationState::new`), and
`association_log.rs:IdentityUpdate::update_state` compares the resulting state's
`inbox_id` against the update's declared `inbox_id`, returning
`AssociationError::WrongInboxId` on mismatch.

### 3.4 Signature kinds and their verification

`crates/xmtp_id/src/associations/signature.rs:SignatureKind`:
`Erc191`, `Erc1271`, `InstallationKey`, `LegacyDelegated`, `P256`.

Dispatch is `unverified.rs:UnverifiedSignature::to_verified`, which matches the
unverified variant and calls a constructor on
`crates/xmtp_id/src/associations/verified_signature.rs:VerifiedSignature`:

| Unverified variant | Verifier fn | What it does | Resulting `kind` / `chain_id` |
| --- | --- | --- | --- |
| `RecoverableEcdsa` | `VerifiedSignature::from_recoverable_ecdsa` | Normalize to lower-s (`signature.rs:to_lower_s`), parse as an alloy `Signature`, `recover_address_from_msg(signature_text)`, format as a hex address. **Recovers** the signer — it does not compare to an expected address. | `Erc191` / `None` |
| `InstallationKey` | `VerifiedSignature::from_installation_key` | `verifying_key.credential_verify::<InstallationKeyContext>(text, sig[..64])` — ed25519 prehashed with SHA-512 and the domain-separation context **`b"IDENTITY UPDATE SIGNATURE"`** (`crates/xmtp_id/src/constants.rs:INSTALLATION_KEY_SIGNATURE_CONTEXT`, marked "DO NOT CHANGE. SIGNATURES WILL BREAK", used via `signature.rs:InstallationKeyContext`). Signer is the verifying key itself. | `InstallationKey` / `None` |
| `SmartContractWallet` | `VerifiedSignature::from_smart_contract_wallet` | Calls the verifier with `eip191_hash_message(signature_text)`; see section 4. | `Erc1271` / `Some(chain_id)` |
| `LegacyDelegated` | `VerifiedSignature::from_legacy_delegated` | See below. | `LegacyDelegated` / `None` |
| `Passkey` | `VerifiedSignature::from_passkey` | See below. | `P256` / `None` |

**Legacy delegated** (`verified_signature.rs:from_legacy_delegated`): recover the signer
of the *delegated key's* signature over the update text; validate the
`SignedPublicKeyProto` into a `ValidatedLegacySignedPublicKey`; derive the delegated
key's address from its sec1 public key and require it to equal the recovered signer;
then report the signer as the **wallet address inside the signed public key**, and use
the *wallet* signature bytes as `raw_bytes` so the legacy key cannot be replayed.
`ValidatedLegacySignedPublicKey::try_from`
(`crates/xmtp_id/src/associations/serialization.rs`) requires a 65-byte wallet signature
(compact 64 + recovery byte), recovers the wallet address over the "XMTP : Create
Identity" text, sanitizes it, and decodes the secp256k1-uncompressed public key.

The exact legacy transcript is built by
`crates/xmtp_id/src/associations/signature.rs` (`header_text`, `body_text`,
`footer_text`, `text`):

```text
XMTP : Create Identity
<hex of serialized legacy key>

For more info: https://xmtp.org/signatures/
```

Two differences from the modern text matter for domain separation: the header and body
are joined by a single `\n` (not `\n\n`), and the footer URL **ends with a trailing
slash** — unlike the modern footer in section 3.2.

**Passkey** (`verified_signature.rs:from_passkey`): parse `client_data_json`; require
`client_data.challenge == base64url_nopad(signature_text)`; parse the P-256 public key
from SEC1 bytes; parse the DER signature (which rejects high-s); verify over
`authenticator_data || sha256(client_data_json)`. The signer identifier carries
`relying_party = Some(client_data.origin)`. Errors: `SignatureError::InvalidClientData`,
`SignatureError::InvalidPublicKey`, `SignatureError::Invalid`.

**The standard WebAuthn checks are absent.** The challenge binding above is the only
security-relevant check. `from_passkey` does **not** validate:

- `clientData.type` — the `ClientDataJson` struct deserializes only `origin` and
  `challenge`, and serde ignores unknown fields, so `"webauthn.create"` is accepted where
  `"webauthn.get"` is expected.
- **origin** — `client_data.origin` is trusted wholesale and stored verbatim. It is never
  compared against an expected origin or against the `relying_party` already recorded for
  that passkey.
- **rpIdHash** — `authenticator_data` is consumed as opaque bytes. Its first 32 bytes are
  never extracted or compared, so nothing binds the assertion to a relying party.
- **flags (UP / UV)** — byte 32 is never parsed. An assertion made with no user
  interaction is accepted.
- **signature counter** — bytes 33..37 are never read or stored. No cloned-authenticator
  detection.

There is also no minimum-length check on `authenticator_data`.

**`relying_party` is not a security boundary.**
`crates/xmtp_id/src/associations/ident/passkey.rs` hand-writes `PartialEq` and `Hash` to
use **only `key`**, and `Display` emits only `hex::encode(&key)`. So `relying_party` is
attacker-supplied, excluded from identity comparison and hashing, and absent from the
signed text. The same key under two different origins is one identity everywhere.

**What `raw_bytes` holds** — this is the replay-dedup key, so normalization matters:

| Constructor | `raw_bytes` stored |
| --- | --- |
| `from_recoverable_ecdsa` | the **lower-s normalized** bytes, not the supplied ones — this closes a malleability replay, since flipping `s` would otherwise yield a second distinct key for one signature |
| `from_installation_key` | the supplied bytes verbatim (ed25519 is non-malleable under strict verification) |
| `from_passkey` | the **parsed** signature re-encoded as fixed 64-byte `r‖s`, not the supplied DER — this canonicalizes away DER slack |
| `from_legacy_delegated` | the **wallet** signature's `raw_bytes` (itself lower-s normalized), so one legacy key can be used only once across the whole log |
| `from_smart_contract_wallet` | the supplied bytes verbatim. ERC-1271 signatures are opaque blobs with no canonical form, so a wallet contract the attacker controls can mint many distinct-but-valid encodings. Dedup is weakest here; `verify_chain_id_matches` is the partial mitigation. |

So "raw bytes" is accurate only for the installation-key and SCW kinds. For ECDSA,
legacy, and passkey the stored value is a **canonicalized** form.

Note `verified_signature.rs:from_recoverable_ecdsa_with_expected_address` exists but is
not used on this path.

### 3.5 State machine rules

`crates/xmtp_id/src/associations/association_log.rs`. `IdentityUpdate::update_state`
applies actions in order and then:

1. Requires the final state to exist, else `AssociationError::NotCreated`.
2. Requires `new_state.inbox_id() == self.inbox_id`, else `AssociationError::WrongInboxId`.
3. Adds every signature of the update to `seen_signatures`
   (`state.rs:AssociationState::add_seen_signatures`).

**Replay protection**: `association_log.rs:IdentityAction::replay_check` (default method)
checks each of the action's stored signature bytes against `state.has_seen(...)` and
returns `AssociationError::Replay`. It is called by `AddAssociation`,
`RevokeAssociation`, and `ChangeRecoveryIdentity` — **not** by `CreateInbox` (there is no
prior state to check against; `MultipleCreate` covers that case instead).

**Ordering: signatures are recorded only after every action is applied.**
`IdentityUpdate::update_state` loops over all actions first and calls
`add_seen_signatures(self.signatures())` once at the end. So the state each per-action
`replay_check` reads never contains any signature from the *current* update. The code
comment scopes the guarantee explicitly to "subsequent updates".

The consequence: **reuse of one signature across several actions inside a single
`IdentityUpdate` is not detected**. What limits this in practice is the signature text —
each action embeds its own target line (section 3.2), and all actions in an update share
one update-level text, so a reused signature can only satisfy actions whose texts
coincide. Duplicate identical actions within one update are the case to think about.

Because seen signatures live in the `AssociationState`, replay protection across updates
only works if the caller supplies the full prior update log as `old_updates`.

Note also that the bytes compared are the **canonicalized** `raw_bytes` from section 3.4,
not the bytes as they arrived on the wire.

Per-action rules:

**CreateInbox** (`association_log.rs:CreateInbox::update_state`)

| Check | Error |
| --- | --- |
| No existing state | `MultipleCreate` ("Multiple create operations detected") |
| Recovered signer == declared `account_identifier` | `MissingExistingMember` |
| Signature kind allowed for identifier kind | `SignatureNotAllowed(role, kind)` |
| If kind is `LegacyDelegated`, `nonce` must be 0 | `LegacySignatureReuse` |
| `AssociationState::new` derives the inbox id | `InvalidAccountAddress` |

The created state has one member (the account identifier), the account identifier as
recovery identifier, and the signature's `chain_id` recorded on that member.

**AddAssociation** (`association_log.rs:AddAssociation::update_state`)

In order:

1. State must exist → else `NotCreated`.
2. `replay_check` → `Replay`.
3. New member signature's recovered signer must equal the declared
   `new_member_identifier` → `NewMemberIdSignatureMismatch`.
4. New member ≠ existing member signer → `Generic("tried to add self")`.
5. **Conditionally**, the legacy nonce-0 rule: if either signature is `LegacyDelegated`
   and the existing state's inbox id is not `identifier.inbox_id(0)` →
   `LegacySignatureReuse`.

   This rule is **not unconditional**. It is guarded by
   `if let Some(identifier) = identifier`, where `identifier` comes from
   `impl From<MemberIdentifier> for Option<Identifier>` (`member.rs`), which returns
   `None` for the `Installation` variant. So **when the existing signer is an
   installation the whole check is skipped**; it runs only when the existing signer is
   Ethereum or passkey. `allowed_signature_for_kind` contains most of the exposure — an
   installation signer must use an `InstallationKey` signature, so it can never itself be
   `LegacyDelegated` — but the condition is an `||` over *both* signatures, so a
   `LegacyDelegated` **new member** signature paired with an installation existing-signer
   bypasses the nonce-0 constraint. Installation-to-Ethereum association is otherwise
   allowed (see step 10).
6. `allowed_signature_for_kind(new_member_identifier.kind(), new_member_signature.kind)`
   → `SignatureNotAllowed`.
7. If the existing signer is already a member, `verify_chain_id_matches` → `ChainIdMismatch(a, b)`
   ("Wrong chain id. Initially added with {0} but now signing from {1}").
8. Determine the authorizing entity: an existing member, else the state's recovery
   identifier. If the signer is neither → `MissingExistingMember`. If the recovery path is
   used with a `LegacyDelegated` signature → `LegacySignatureReuse`.
9. `allowed_signature_for_kind` for the existing entity too → `SignatureNotAllowed`.
10. `allowed_association(existing_kind, new_kind)` → `MemberNotAllowed(a, b)`. The only
    forbidden pair is Installation adding Installation.
11. Insert `Member::new(new_member, Some(existing_entity_id), Some(client_timestamp_ns), new_member_signature.chain_id)`.

**RevokeAssociation** (`association_log.rs:RevokeAssociation::update_state`)

1. State must exist → `NotCreated`. 2. `replay_check`. 3. `verify_chain_id_matches` if
the signer is a member. 4. Signature must not be `LegacyDelegated` →
`SignatureNotAllowed("ethereum", "legacy-delegated")`. 5. Signer must equal the state's
recovery identifier → `MissingExistingMember`. 6. Remove the revoked member **and** every
installation-kind member whose `added_by_entity` is that member
(`state.rs:AssociationState::members_by_parent`). Revocation is idempotent, hence the
code comment that no replay check is needed for the removal itself.

**ChangeRecoveryIdentity** (`association_log.rs:ChangeRecoveryIdentity::update_state`)

Same guards (`NotCreated`, `replay_check`, chain-id match, no legacy signature, signer
must be the current recovery identifier), then `set_recovery_identifier`.

**Signature-kind matrix** (`association_log.rs:allowed_signature_for_kind`):

| Member kind | Allowed signature kinds |
| --- | --- |
| `Ethereum` | `Erc191`, `Erc1271`, `LegacyDelegated` |
| `Installation` | `InstallationKey` |
| `Passkey` | `P256` |

**Full `AssociationError` table** (`association_log.rs:AssociationError`):

| Variant | Message |
| --- | --- |
| `Generic(String)` | `Error creating association {0}` |
| `MultipleCreate` | `Multiple create operations detected` |
| `NotCreated` | `XID not yet created` |
| `Signature(SignatureError)` | `Signature validation failed {0}` |
| `MemberNotAllowed(a, b)` | `Member of kind {0} not allowed to add {1}` |
| `MissingExistingMember` | `Missing existing member` |
| `LegacySignatureReuse` | `Legacy key is only allowed to be associated using a legacy signature with nonce 0` |
| `NewMemberIdSignatureMismatch` | `The new member identifier does not match the signer` |
| `WrongInboxId` | `Wrong inbox_id specified on association` |
| `SignatureNotAllowed(role, kind)` | `Signature not allowed for role {0:?} {1:?}` |
| `Replay` | `Replay detected` |
| `Deserialization(..)` | `Deserialization error {0}` |
| `MissingIdentityUpdate` | `Missing identity update` |
| `ChainIdMismatch(a, b)` | `Wrong chain id. Initially added with {0} but now signing from {1}` |
| `InvalidAccountAddress` | `Invalid account address: Must be 42 hex characters, starting with '0x'.` |
| `NotIdentifier(String)` | `{0} are not a public identifier` |
| `Convert(..)` | transparent |

Only `AssociationError::Signature(e)` can be retryable, and only when the inner
`SignatureError` is (`handlers.rs:impl RetryableError for GrpcServerError`).

### 3.6 Backend vs client responsibilities

**The backend does** (via `GetAssociationState`): verify every signature on every update
(including chain calls for SCW), run the full state machine over `old_updates` and then
`new_updates`, enforce replay/inbox-id/recovery/chain-id rules, and return the resulting
state and diff. It is the authority on whether an identity update may be published and
on which installations belong to an inbox.

**The client also does** the same verification locally — `xmtp_mls` builds and applies
the same `IdentityUpdate`s using the same `xmtp_id::associations` code with a
`RemoteSignatureVerifier` as its SCW verifier. So the logic is shared, not
backend-exclusive.

**The client alone does** identity-update *construction*: collecting signatures against
the signature text and assembling the update. That is
`crates/xmtp_id/src/associations/builder.rs` — `SignatureRequestBuilder` (methods
`create_inbox`, `add_association`, `revoke_association`, `change_recovery_address`,
`build`) and `SignatureRequest` (`add_signature`,
`add_new_unverified_smart_contract_signature`, `missing_signatures`, `is_ready`,
`build_identity_update`). `SignatureRequest::add_verified_signature` rejects a signature
from a signer that is not in `missing_signatures` with
`SignatureRequestError::UnknownSigner`, and
`add_new_unverified_smart_contract_signature` requires a resolved block number
(`SignatureRequestError::BlockNumber`). None of that runs on the backend.

**Network dependencies of the backend:** only the chain RPC, and only when an update
contains an `Erc1271` signature. Everything else is pure CPU.

---

## 4. Smart contract wallet verification

### 4.1 The trait

`/Users/nickmolnar/code/xmtp/libxmtp/crates/xmtp_id/src/scw_verifier/mod.rs:SmartContractSignatureVerifier`:

```rust
async fn is_valid_signature(
    &self,
    account_id: AccountId,
    hash: [u8; 32],
    signature: Bytes,
    block_number: Option<BlockNumber>,
) -> Result<ValidationResponse, VerifierError>;
```

Blanket impls exist for `Arc<T>`, `&T`, and `Box<T>` in the same file.

`scw_verifier/mod.rs:ValidationResponse` has three fields: `is_valid: bool`,
`block_number: Option<u64>`, `error: Option<String>`.

### 4.2 `AccountId`

`crates/xmtp_id/src/associations/signature.rs:AccountId` is CAIP-10: `chain_id: String`
(for example `"eip155:1"`) and `account_address: String`. `AccountId::new_evm(chain_id: u64, addr)`
formats `"eip155:{chain_id}"`. `AccountId::get_chain_id_u64` strips the `eip155:` prefix
(else `AccountIdError::MissingEip155Prefix`, "Chain ID is not prefixed with eip155:") and
parses a `u64` (else `AccountIdError::InvalidChainId`, "Chain ID is not a valid u64").
A malformed proto account id becomes `DeserializationError::InvalidAccountId` in the
handler.

### 4.3 The on-chain check

`crates/xmtp_id/src/scw_verifier/chain_rpc_verifier.rs:RpcSmartContractWalletVerifier::is_valid_signature`:

1. Decode the ERC-6492 off-chain validator bytecode from
   `crates/xmtp_id/src/scw_verifier/signature_validation.hex` (the AmbireTech
   `ValidateSigOffchain` contract, per the file's own comment — "not a complete ERC-6492
   implementation as it lacks Prepare/Side-effect logic").
2. Parse `account_address` into an `Address`; failure →
   `VerifierError::FromHex(FromHexError::InvalidStringLength)`.
3. ABI-encode the constructor call `VerifySig(address _signer, bytes32 _hash, bytes _signature)`
   and concatenate it after the bytecode. This is the standard "deployless call" pattern:
   the constructor runs and returns the result.
4. Resolve the block: use the supplied `block_number`, or call `provider.get_block_number()`
   when it is `None`. RPC failure → `VerifierError::Provider`.
5. `provider.call(tx).block(block_number.into())` — an `eth_call` **pinned to that block**.
6. `is_valid = (result == 0x01)`.
7. Return `ValidationResponse { is_valid, block_number: Some(block_number), error: None }`.

**Block-number semantics.** The block number is the point in chain history at which the
signature was judged valid. Passing `None` means "evaluate at current head, and tell me
which block that was" — **but only on a cache miss**; see section 4.6, where the cache
key is built from the *input* block number, so a later `None` call can return an older
block's verdict. The response always carries a concrete `Some(block)`. This
matters because a smart wallet's owner set can change: a signature valid at block N may
be invalid at block N+1. `verified_signature.rs:from_smart_contract_wallet` writes the
resolved block back through its `&mut Option<u64>` argument so the caller can persist it,
and `builder.rs:SignatureRequest::add_new_unverified_smart_contract_signature` refuses to
proceed if it is still `None`. Once stored in an `IdentityUpdate`, the block number is
replayed on every future `GetAssociationState`, so revalidation is deterministic — the
node re-checks at the original block, not at head. The block number is also part of the
verifier cache key.

### 4.4 Chain routing and configuration

`crates/xmtp_id/src/scw_verifier/mod.rs:MultiSmartContractSignatureVerifier` holds
`HashMap<String, Box<dyn SmartContractSignatureVerifier>>` keyed by the CAIP-2 chain id
string. `impl SmartContractSignatureVerifier for MultiSmartContractSignatureVerifier`
looks up `account_id.chain_id` and, when absent, returns
`VerifierError::NoVerifier(chain_id)` ("verifier not present for chain ID {0}").

Constructors:

| Fn | Behavior |
| --- | --- |
| `MultiSmartContractSignatureVerifier::new(HashMap<String, Url>)` | One `RpcSmartContractWalletVerifier` per url. |
| `new_providers(HashMap<String, DynProvider>)` | For tests / preconstructed providers. |
| `new_from_file(path)` | Read the JSON file, parse `HashMap<String, Url>`, then `new`. **Does not** call `upgrade`, so no env overrides and no anvil. |
| `new_from_env()` | Parse the embedded default JSON, then `upgrade()`. |

`DEFAULT_CHAIN_URLS` is `include_str!("chain_urls_default.json")`. That file
(`crates/xmtp_id/src/scw_verifier/chain_urls_default.json`) currently maps 11 chains:

```json
{
  "eip155:1": "https://ethereum-rpc.publicnode.com",
  "eip155:10": "https://mainnet.optimism.io",
  "eip155:137": "https://polygon.publicnode.com",
  "eip155:324": "https://mainnet.era.zksync.io",
  "eip155:8453": "https://mainnet.base.org",
  "eip155:42161": "https://arb1.arbitrum.io/rpc",
  "eip155:59144": "https://linea-rpc.publicnode.com",
  "eip155:480": "https://worldchain-mainnet.g.alchemy.com/public",
  "eip155:232": "https://rpc.lens.xyz",
  "eip155:2741": "https://api.mainnet.abs.xyz",
  "eip155:100": "https://rpc.gnosischain.com"
}
```

`MultiSmartContractSignatureVerifier::upgrade` then, for each configured chain id, reads
env var `CHAIN_RPC_<numeric id>` (for example `CHAIN_RPC_1`, `CHAIN_RPC_8453`) and
replaces the default url when present.

**`upgrade` does not check the `eip155:` namespace.** Despite `MalformedEipUrl`'s message
("Chain IDs must be preceded with eip155:"), the code only does `id.split(":").nth(1)` and
never compares the first component against `"eip155"`. So `MalformedEipUrl` is raised only
when the key has **no colon at all**. Values such as `foo:1` or `solana:5eykt4Us` pass
this step, and the second component is used to build the env var name — meaning `eip155:1`
and `otherns:1` both read `CHAIN_RPC_1`, and a key with extra colons keeps only its second
component. It finally registers a local chain under
`eip155:31337` from `ANVIL_URL`, or from `xmtp_configuration::DockerUrls::ANVIL` when
that env var is unset (`add_anvil`).

So: `--chain-urls <file>` gives an exact, closed set with **no** env overrides and **no**
anvil; omitting the flag gives the built-in list plus `CHAIN_RPC_*` overrides plus anvil.

### 4.5 `VerifierError` and retryability

`scw_verifier/mod.rs:VerifierError`:

| Variant | Message | Retryable |
| --- | --- | --- |
| `UnexpectedERC6492Result(String)` | `unexpected result from ERC-6492 {0}` | no |
| `FromHex(hex::FromHexError)` | transparent | no |
| `Provider(RpcError<TransportErrorKind>)` | transparent | **yes** |
| `Url(url::ParseError)` | transparent | no |
| `Io(std::io::Error)` | transparent | **yes** |
| `Serde(serde_json::Error)` | transparent | no |
| `MalformedEipUrl` | `Chain IDs must be preceded with eip155:` | no |
| `NoVerifier(String)` | `verifier not present for chain ID {0}` | **yes** |
| `InvalidHash(Vec<u8>)` | `hash was invalid length or otherwise malformed` | no |
| `Other(Box<dyn RetryableError>)` | `{0}` | delegates |

`scw_verifier/mod.rs:impl RetryableError for VerifierError` defines that column.
`signature.rs:impl RetryableError for SignatureError` propagates only the
`VerifierError` case; every other `SignatureError` is terminal. The code comment there
cites xmtp/libxmtp#3394: a transient RPC failure must not permanently advance the
welcome-sync cursor past welcomes involving SCW users.

The retryable set is exactly **`Provider`, `Io`, `NoVerifier`, and `Other`** (the last
delegating to its inner error). Note that this is not "only chain-RPC trouble":
`NoVerifier` is a purely **local routing and configuration** failure — the chain id is
simply absent from the map — and `Io` need not involve the chain either. A `_ => false`
catch-all covers the rest, so any variant added later defaults to non-retryable with no
compile error.

Net effect at the RPC boundary: a chain RPC outage or an unconfigured chain surfaces to
the caller of `GetAssociationState` as gRPC `Unavailable`, while a genuinely invalid
signature surfaces as `InvalidArgument`. `signature.rs:tests` pins both directions
(`test_signature_error_verifier_retryable_propagates`,
`test_signature_error_verifier_non_retryable_propagates`).

### 4.6 Caching

`apps/mls_validation_service/src/cached_signature_verifier.rs:CachedSmartContractSignatureVerifier`
wraps any verifier with a `parking_lot::Mutex<LruCache<[u8;32], ValidationResponse>>`
sized by `--cache-size` (default 10000).

The key is built by `cached_signature_verifier.rs:build_cache_key`: keccak256 over a
length-prefixed encoding of `chain_id`, `account_address`, `hash`, `signature`, and a
tagged `block_number` (`0x01 || be_bytes` for `Some`, `0x00` for `None`). Length
prefixes make the encoding unambiguous, and `Some(0)` differs from `None`. This is a fix
for xmtp/libxmtp#3393 (cross-account cache poisoning), regression-tested by
`cached_signature_verifier.rs:tests::test_cache_key_includes_all_params`.

Only successes and negative results are cached — the code caches whatever
`ValidationResponse` comes back, and returns early on a hit. Errors are not cached
(the `?` propagates before the `cache.put`).

Cache is in-memory and per-process. It is dropped on restart.

**There is no TTL, and the key uses the input block number.** Both points matter for the
new crate:

- `LruCache::new(cache_size)` has no expiry concept. There is no time value anywhere in
  the non-test code and no background eviction. **Entries are evicted only by LRU
  capacity pressure, never by age.**
- `build_cache_key` is given the caller-supplied `block_number` *before* the downstream
  call resolves it. `None` is encoded as the single tag byte `0x00`. The resolved block
  that comes back in `ValidationResponse.block_number` is **never folded into the key**;
  the response is stored under the `None`-tagged key.

Together these give stale state on repeated calls. A `block_number: None` request means
"verify against latest chain state", but every such request for one
(chain, account, hash, signature) tuple collapses to a single key. The first call hits the
chain at block N and pins that verdict; every later `None` call returns the block-N answer
without touching the chain, for as long as the entry survives. The returned
`block_number` still reports the stale N, so callers cannot tell.

This is a correctness issue because ERC-1271/ERC-6492 validity is **mutable on-chain
state**. An owner rotation, signer removal, or wallet upgrade flips a signature's
validity, and a cached `true` outlives the revocation — bounded only by LRU eviction,
which a caller that keeps querying actively prevents. A cached negative is equally sticky
across a legitimate wallet deployment. Keying on the *resolved* block, or applying a TTL
to `None`-keyed entries, would each fix this.

### 4.7 How clients reach it

Clients do **not** call the validation service. In v3 they call the **node's** identity
API, which proxies to the validation service. In d14n they do the chain call themselves.

Two client-side implementations of the trait exist, with identical bodies:

- `crates/xmtp_id/src/scw_verifier/remote_signature_verifier.rs:RemoteSignatureVerifier<C>`
  — generic over `C: XmtpIdentityClient`. **This is dead code.** A repo-wide search finds
  no construction site outside its own file; it is only re-exported by
  `scw_verifier/mod.rs`.
- `crates/xmtp_api/src/scw_verifier.rs:impl SmartContractSignatureVerifier for ApiClientWrapper<C>`
  — the live path. It goes through
  `crates/xmtp_api/src/identity.rs:ApiClientWrapper::verify_smart_contract_wallet_signatures`,
  which wraps the call in `retry_async!(self.retry_strategy, ...)`.

Both build a `VerifySmartContractWalletSignaturesRequest` with exactly one signature
(`account_id.into()`, `block_number`, `signature.to_vec()`, `hash.to_vec()`) and take
`responses.into_iter().next()`. An empty response list becomes
`VerifierError::Io(InvalidData, "API returned empty response for signature verification request")`
— which is **retryable**. Transport errors become `VerifierError::Other(Box::new(e))`,
which delegates retryability to the wrapped API error.

Wiring: `crates/xmtp_mls/src/builder.rs:ClientBuilder::with_remote_verifier` installs
`Box::new(ApiClientWrapper::new(api, Retry::default()))` as the client's verifier. All
three bindings call it (`bindings/wasm/src/client.rs`, `bindings/mobile/src/mls.rs`,
`bindings/node/src/client/create_client.rs`). The alternative
`ClientBuilder::with_scw_verifier` is used almost only by tests with
`MockSmartContractSignatureVerifier`. At runtime the verifier is read via
`crates/xmtp_mls/src/client.rs:Client::scw_verifier`.

Two transport paths follow from there:

- **v3 / node-go:** `crates/xmtp_api_d14n/src/queries/v3/identity.rs` issues gRPC
  `"/xmtp.identity.api.v1.IdentityApi/VerifySmartContractWalletSignatures"`
  (`crates/xmtp_api_d14n/src/endpoints/v3/identity/verify_smart_contract_wallet_signatures.rs:Endpoint::grpc_endpoint`).
  The node then forwards to the validation service. Full chain: client → node identity API
  → node's mlsvalidate client → validation service → chain RPC.
- **d14n / xmtpd:** **there is no remote hop.**
  `crates/xmtp_api_d14n/src/queries/d14n/identity.rs:verify_smart_contract_wallet_signatures`
  converts each `account_id` and hash locally (a bad hash becomes
  `VerifierError::InvalidHash`) and calls its own
  `Arc<MultiSmartContractSignatureVerifier>`, built by
  `MultiSmartContractSignatureVerifier::new_from_env()` in
  `crates/xmtp_api_d14n/src/queries/d14n/client.rs:D14nClient::new`. So in d14n mode the
  **client device** talks directly to the public chain RPC endpoints in
  `chain_urls_default.json`. `crates/xmtp_api_d14n/src/queries/combined.rs` forwards to
  whichever backend `choose_client()` picks.

### 4.8 Gotchas the crate implementer should know

1. **`new_from_file` skips `upgrade()`.** Running with `--chain-urls` silently disables
   every `CHAIN_RPC_*` override *and* omits the `eip155:31337` anvil verifier that
   `new_from_env` always installs. Easy to trip over in local and test deployments.
2. **`NoVerifier` is classified retryable.** A permanently unsupported chain is retried by
   `retry_async!` and surfaced to callers as `Unavailable`, so it looks like a transient
   outage rather than a configuration error.
3. **`VerifierError::UnexpectedERC6492Result` is never constructed** anywhere in the repo.
4. **`RemoteSignatureVerifier` duplicates the `ApiClientWrapper` impl verbatim.** Two
   copies of the same logic, one unreachable.
5. **The default chain URLs are free public endpoints** embedded with `include_str!`.
   Without `CHAIN_RPC_*` overrides, SCW verification depends on rate-limited public RPC —
   and in d14n mode that happens from the client device, not from a server.
6. `AccountId`'s `TryFrom<String>` (`crates/xmtp_id/src/associations/serialization.rs`)
   is generic CAIP-10, not EVM-only: it requires exactly three colon-separated parts and
   validates chain id against `^[-a-z0-9]{3,8}:[-_a-zA-Z0-9]{1,32}$` and address against
   `^[-.%a-zA-Z0-9]{1,128}$`. Tests round-trip `bip122:`, `cosmos:`, `polkadot:`,
   `starknet:` and `hedera:` ids.

---

## 5. Backend wiring

The two backends use the service very differently. Both consume the same four-field
`ValidateGroupMessagesResponse`; the Go generated protos in both repos are identical to
the Rust one (`/Users/nickmolnar/code/xmtp/xmtp-node-go/pkg/proto/mls_validation/v1/service.pb.go:ValidateGroupMessagesResponse_ValidationResponse`
and `/Users/nickmolnar/code/xmtp/xmtpd/pkg/proto/mls_validation/v1/service.pb.go:ValidateGroupMessagesResponse_ValidationResponse`
— `IsOk`, `ErrorMessage`, `GroupId`, `IsCommit`, and nothing else).

### 5.1 xmtp-node-go (v3)

Client: `/Users/nickmolnar/code/xmtp/xmtp-node-go/pkg/mlsvalidate/service.go:NewMlsValidationService`
— dials with the deprecated `grpc.DialContext` plus `insecure.NewCredentials()`. Plaintext
only, no interceptors, no metrics, no keepalive, and **no call-size options**, so grpc-go
v1.53.0's asymmetric client defaults apply
(`/Users/nickmolnar/go/pkg/mod/google.golang.org/grpc@v1.53.0/clientconn.go`):
`defaultClientMaxSendMessageSize = math.MaxInt32` (about 2 GiB, effectively unlimited) and
`defaultClientMaxReceiveMessageSize = 4 MiB`. These are **client** limits on what this
process will send and accept, not a statement about what the server accepts — see the
request-size note at the end of section 5.3.

Config: `/Users/nickmolnar/code/xmtp/xmtp-node-go/pkg/mlsvalidate/config.go:MLSValidationOptions`
has one field, `GRPCAddress`, namespaced at
`/Users/nickmolnar/code/xmtp/xmtp-node-go/pkg/server/options.go` as
`--mls-validation.grpc-address`. **There is no env-var tag**, so it is CLI-flag only.
`/Users/nickmolnar/code/xmtp/xmtp-node-go/pkg/server/server.go` only constructs the
validator when the address is non-empty; when empty,
`/Users/nickmolnar/code/xmtp/xmtp-node-go/pkg/api/server.go` silently skips registering
the MLS and Identity gRPC servers, so misconfiguration shows up as `Unimplemented`, not
a startup failure.

**Fail-fast batch handling.** All three batch methods
(`service.go:MLSValidationServiceImpl.ValidateInboxIdKeyPackages`,
`.ValidateGroupMessages`, `.ValidateGroupMessagePayloads`) collapse the first
`!IsOk` element into `fmt.Errorf("validation failed with error %s", response.ErrorMessage)`.
Per-element results and the distinction between "invalid input" and "service down" are
both lost at this line.

| # | Call site | RPC | Input | Use of result | Failure status |
| --- | --- | --- | --- | --- | --- |
| A1 | `pkg/mls/api/v1/service.go:Service.RegisterInstallation` (deprecated) | `ValidateInboxIdKeyPackages` | one key package, `is_inbox_id_credential=true` | `InstallationKey` → `CreateOrUpdateInstallation`, echoed to client. `Expiration`, `Credential` ignored. | `InvalidArgument` "invalid identity: %s"; `Internal` if `len(results) != 1` |
| A2 | `pkg/mls/api/v1/service.go:Service.UploadKeyPackage` | `ValidateInboxIdKeyPackages` | one key package | `InstallationKey` → `CreateOrUpdateInstallation` | `InvalidArgument`; `Internal` on DB error. **No length guard before `validationResults[0]`** |
| A3 | `pkg/mls/api/v1/service.go:Service.SendGroupMessages` | `ValidateGroupMessages` | all `req.Messages`, using `GetV1().Data` | `GroupId` and `IsCommit` — see below | `InvalidArgument` "invalid group message: %s" |
| A4 | `pkg/mls/store/backfiller_group_messages.go:IsCommitBackfiller.classifyMessageBatch` | `ValidateGroupMessages` | stored `data` rows where `is_commit IS NULL` | only `IsCommit` → `UpdateIsCommitStatus` | `Internal`; also `Internal` when `len(results) != len(messages)` |
| A5 | `pkg/mls/store/store.go:Store.PublishIdentityUpdate` | `GetAssociationState` | full inbox log as `oldUpdates`, the incoming update as `newUpdates` | only `StateDiff` — see below | error returned **bare**, upstream code **preserved** |
| A6 | `pkg/identity/api/v1/identity_service.go:Service.VerifySmartContractWalletSignatures` | `VerifySmartContractWalletSignatures` | request forwarded unchanged | response forwarded unchanged | bare, upstream code **preserved** |

**Upstream status codes survive A5 and A6.** An earlier draft of this page said these
became `codes.Unknown`. That is wrong. Both paths return the error **bare** — no
`fmt.Errorf` wrapping — through `pkg/mlsvalidate/service.go`, which is itself a bare
passthrough, and through `RunInRepeatableReadTx`, which returns `err` unchanged. grpc-go
converts a returned error with `status.FromError`, which honors any error implementing
`GRPCStatus()`, and a `*status.Error` from the upstream call does. So the validation
service's `InvalidArgument` and `Unavailable` reach the client intact. Only a local,
non-status error becomes `Unknown`.

Wrapping with `fmt.Errorf("...: %w", err)` **would** break this on the pinned grpc-go
v1.53.0, whose `status.FromError` does not unwrap. A3 shows the contrast: it flattens
every `ValidateGroupMessages` error — transport failures included — into
`codes.InvalidArgument` with `status.Errorf`, which does destroy the upstream code. Its
in-code TODO acknowledges exactly this.

**How A3 uses `group_id` — this is the load-bearing behavior.** The client never supplies
a group id. The node takes `result.GroupId` (a hex string), checks it is non-empty
(`requireReadyToSend`), `hex.DecodeString`s it (failure → `InvalidArgument` "invalid
group id"), and passes the bytes to `writerStore.InsertGroupMessage`, where they become
the `group_id` column and part of the dedup hash `sha256(groupId || data)`
(`/Users/nickmolnar/code/xmtp/xmtp-node-go/pkg/mls/store/store.go:InsertGroupMessage`).
`result.IsCommit` is passed to the same call and persisted to the `is_commit` column.

**What this does and does not stop.** Deriving the group id from the ciphertext rather
than from a client field stops a client from *mislabeling* a message it did author. It
does **not** stop a client from writing into a group it does not belong to: the client
also controls the serialized message, so it chooses the group id the parse yields, and the
service cannot check membership (section 2.2). Any client can therefore publish to any
group id. As explained in section 2.2, this is an inherent property of MLS
confidentiality, not a defect in this code — the payload stays unreadable and unforgeable
to non-members, and the residual effect is topic spam.

Results are consumed **positionally** (`for i, result := range validationResults { input := req.Messages[i] }`)
with **no length check** — a short response silently drops messages while returning
success; a long one would panic. The backfiller (A4) has the check the API path lacks.
A3 also carries two in-code TODOs: "Separate validation errors from internal errors" and
"Wrap this in a transaction so publishing is all or nothing".

**How A5 uses `StateDiff`.** The validation service is passed *into the store layer* so
the RPC happens inside a `RunInRepeatableReadTx(ctx, 3, ...)` transaction holding a
`pg_advisory_lock` on the inbox id. `StateDiff.NewMembers` filtered to
`MemberIdentifier_EthereumAddress` become `InsertAddressLog` rows;
`StateDiff.RemovedMembers` become `RevokeAddressFromLog` rows. Non-Ethereum member kinds
(installations, passkeys) are silently skipped by the type switch. **`AssociationState`
itself is never used — only the diff.** The full inbox log (capped at 256 entries) is
re-sent on every publish. `state.StateDiff` is dereferenced without a nil check.

Timeouts, retries, caching: **no RPC deadline is ever set** in this repo. Every call
inherits the inbound request context; A4 uses the long-lived server context, so a hung
call blocks that goroutine until shutdown. The only retry is A5's transaction-level loop,
which re-issues the RPC as a side effect: `RunInRepeatableReadTx(ctx, 3, ...)` makes
**3 total attempts** (not 3 retries after a first try) and sleeps `utils.RandomSleep(20)`
between them — `rand.Intn(20)`, so a uniform **0–19 ms**, which can be 0. Worst-case total
backoff is about 38 ms. The sleep also runs after the final failed attempt. Note the
retry is unconditional: it fires for every error, including terminal validation failures.
The only size management is A4's client-side 4 MiB batching.

`ValidateGroupMessagePayloads` has **no production call site** — only a test mock.

### 5.2 xmtpd (d14n)

Client: `/Users/nickmolnar/code/xmtp/xmtpd/pkg/mlsvalidate/service.go:NewMLSValidationService`
— dials via `utils.NewGRPCConn` with Prometheus unary and stream client interceptors, and
closes the connection on `ctx.Done()`. `utils.NewGRPCConn`
(`/Users/nickmolnar/code/xmtp/xmtpd/pkg/utils/api_clients.go`) sets `MaxCallSendMsgSize`
and `MaxCallRecvMsgSize` to `constants.GRPCPayloadLimit` = 25 MiB
(`pkg/constants/constants.go`).

**These are client call options only and cannot raise the server's limit.**
`grpc.WithDefaultCallOptions` is a dial option on the Go `ClientConn`. It has no wire
representation and is never sent to the peer. The tonic server still decodes at its own
4 MiB default, so a request over 4 MiB is rejected with `ResourceExhausted` regardless;
the 25 MiB setting only means this client will not reject it locally first. See section
5.3 — the effective request limit is 4 MiB for **both** callers.

Config: `/Users/nickmolnar/code/xmtp/xmtpd/pkg/config/options.go:MlsValidationOptions.GrpcAddress`,
flag `--mls-validation.grpc-address`, env **`XMTPD_MLS_VALIDATION_GRPC_ADDRESS`**. The
value is a **URL** (`http://localhost:60051`), and the scheme selects TLS vs h2c.
`/Users/nickmolnar/code/xmtp/xmtpd/pkg/config/validation.go` makes it **required** when
either the API or the indexer is enabled, so misconfiguration fails at startup.

**Only two RPCs are live in production.** Grep confirms `ValidateGroupMessages` and the
direct `GetAssociationState` have no production callers in this repo; the live surface is
`ValidateInboxIdKeyPackages` (called through the Go method named `ValidateKeyPackages`)
and `GetAssociationState` (reached only via `GetAssociationStateFromEnvelopes`).

| # | Call site | RPC | Input | Use of result | Failure status |
| --- | --- | --- | --- | --- | --- |
| B1 | `pkg/api/message/service.go:Service.validateKeyPackage`, from `PublishPayerEnvelopes` | `ValidateInboxIdKeyPackages` | one key package from `UploadKeyPackage` payload | **Nothing is extracted.** Only `IsOk` is checked. `InstallationKey`, `Expiration`, `Credential` all discarded. | RPC error → `CodeInternal`; `!IsOk` → `CodeInvalidArgument` with the per-element `ErrorMessage`; empty response → `CodeInternal` |
| B2 | `pkg/indexer/app_chain/contracts/identity_update_storer.go:IdentityUpdateStorer.validateIdentityUpdate` | `GetAssociationState` (via `GetAssociationStateFromEnvelopes`) | up to 256 prior identity-update envelopes as `oldUpdates`, the indexed update as `newUpdates` | `StateDiff` only, batched into `InsertAddressLogsBatch` / `RevokeAddressFromLogBatch` | not an API path; classified retryable vs terminal (below) |

`/Users/nickmolnar/code/xmtp/xmtpd/pkg/api/message/service.go:Service.validateGroupMessage`
does **not** call the validation service despite the name — it only checks
`deserializer.ShouldSendToBlockchain(payload)` locally and rejects commits and proposals
with "commit and proposal messages must be published via the blockchain". That is why
xmtpd needs neither `group_id` (topics come from the client envelope, checked by
`clientEnv.TopicMatchesPayload()`) nor `is_commit` — `GroupMessageValidationResult` in
`/Users/nickmolnar/code/xmtp/xmtpd/pkg/mlsvalidate/interface.go` does not even have an
`IsCommit` field.

**Better error separation.** `service.go:MLSValidationServiceImpl.ValidateKeyPackages`
preserves per-element `IsOk` and `ErrorMessage` instead of failing fast, which lets B1
map transport errors to `CodeInternal` and validation failures to `CodeInvalidArgument`.
This is exactly the separation node-go's TODO asks for. `ValidateGroupMessages` in the
same file still fails fast, but it is unused.

**Error classification by substring — the most important consequence of the current
design.** `identity_update_storer.go:shouldRetryValidationError` decides whether an
indexing failure is permanent or should be retried by matching the error string against
`associationErrorPatterns`, a hardcoded list of **17 substrings copied by hand from
libxmtp** (`identity_update_storer.go`): "Error creating association", "Multiple create
operations detected", "XID not yet created", "Signature validation failed", "not allowed
to add", "Missing existing member", "Legacy key is only allowed to be associated using a
legacy signature with nonce 0", "The new member identifier does not match the signer",
"Wrong inbox_id specified on association", "Signature not allowed for role", "Replay
detected", "Deserialization error", "Missing identity update", "Wrong chain id.",
"Invalid account address: Must be 42 hex characters, starting with '0x'.", "are not a
public identifier", and **"Conversion error"**. A match means non-recoverable; no match
means retry. The code carries its own critique: "this approach is fragile as it depends on
us creating new error messages for new validation errors. This function should rely on
gRPC error codes instead, but it's not possible at the moment", citing xmtp/libxmtp#3130.
The default is *retry*, so changing any of those strings in libxmtp silently converts a
permanent rejection into an infinite indexer retry loop.

**One pattern is already stale: `"Conversion error"` matches nothing.** Two reasons.
`AssociationError::Convert` is `#[error(transparent)]`, so it emits the inner error's
Display with no prefix at all. And no `ConversionError` variant's Display contains that
substring — the strings are "missing field … during conversion from protobuf", "field {}
unspecified", "decoding proto {0}", and so on (section 1). Since section 2.3 established
that malformed identity-update protos produce exactly this `ConversionError` path, a
terminal proto error is **classified retryable by default** and spins in B2's loop
forever. The neighbouring patterns "Signature validation failed" and "Deserialization
error" are fine, because those variants keep a literal prefix.

Note that **only xmtpd does this substring classification**; node-go has no equivalent.

B2 runs inside `db.RunInTx` at `sql.LevelReadCommitted` and uses nil-safe getters
`GetNewMembers()` / `GetRemovedMembers()`, so it does not have node-go's nil-`StateDiff`
panic risk.

**xmtpd does take an advisory lock.** An earlier draft said the chain's ordering made one
unnecessary. That is wrong. Inside the transaction body,
`identity_update_storer.go` calls
`db.NewAdvisoryLocker().LockIdentityUpdateInsert(ctx, querier, IdentityUpdateOriginatorID)`
**before** it reads state, which runs `SELECT pg_advisory_xact_lock($1)`
(`/Users/nickmolnar/code/xmtp/xmtpd/pkg/db/queries/advisory_locks.sql.go`). The lock is
transaction-scoped, so it releases on commit or rollback, and its key is
`(nodeID << 8) | LockKindIdentityUpdateInsert`. A long comment in the file explains the
pairing: the lock stops two HA workers from processing one event at once, and READ
COMMITTED (rather than REPEATABLE READ) is deliberate so that `GetLatestSequenceId` sees
rows committed while this worker waited on the lock.

Timeouts, retries, caching: **no deadline** on these RPCs. The 10-second `clientTimeout`
in `pkg/utils/api_clients.go` applies only to the Connect HTTP client, not to
`NewGRPCConn`'s dialer. The only retry is B2's recoverable-error loop in
`/Users/nickmolnar/code/xmtp/xmtpd/pkg/indexer/common/log_handler.go:retry`, which is an
unbounded `for {}` with a **fixed 100 ms** sleep and **no backoff** — it exits only on
`ctx.Done()` or a non-retryable error. Because `retry` is called inline in the event loop,
a mis-classified terminal error (see the `"Conversion error"` gap above) blocks that
contract's whole log pipeline at ten attempts per second indefinitely.

### 5.3 What this means for the new crate

- `group_id` from `ValidateGroupMessages` must be derived from the ciphertext, never from
  a separate client field — node-go uses it as the DB routing key and dedup input. But be
  precise about what that buys: it stops a client mislabeling its own message; it does
  **not** authorize the sender, and any client can still publish to any group id
  (sections 2.2 and 5.1).
- No backend needs `epoch` or `sender`. Neither field exists anywhere in the contract.
- The batch contract is positional and unchecked by node-go's API path: response `i` must
  correspond to request `i`, and the response length must equal the request length.
- xmtpd classifies errors by **string matching**, so changing an error message in
  `association_log.rs` is a breaking change for its indexer — and one of its 17 patterns
  is already dead. Exposing gRPC status codes or the `error-code` metadata properly
  (libxmtp#3130) is the fix.
- **The effective request limit is 4 MiB for both callers.** There is no real 4-vs-25 MiB
  disagreement to reconcile: tonic's generated server decodes at a 4 MiB default
  (`crates/xmtp_proto/src/gen/xmtp.mls_validation.v1.rs`) which `main.rs` never overrides,
  and a Go client's `MaxCallSendMsgSize` cannot raise a server-side limit. xmtpd's 25 MiB
  only relaxes its own local check. The new backend should still state its request-size
  limit explicitly, and should note that the generated server's
  `max_encoding_message_size` defaults to `usize::MAX` — responses are unbounded.

---

## 6. Configuration, health check, packaging

### 6.1 CLI flags

`apps/mls_validation_service/src/config.rs:Args`:

| Flag | Default | Meaning |
| --- | --- | --- |
| `-v`, `--version` | `false` | Print version and exit. |
| `-p`, `--port` | `50051` | gRPC port. Bound as `0.0.0.0:<port>`. |
| `--health-check-port` | `50052` | HTTP health port. |
| `--chain-urls <path>` | none | JSON file, same shape as `chain_urls_default.json`. When set, `new_from_file` is used and env overrides / anvil are skipped. |
| `--cache-size` | `10000` | LRU size for the SCW verifier cache. `NonZeroUsize`. |
| `--log-format` (env `LOG_FORMAT`) | `text` | `text` or `json`. |

Env vars read indirectly by the verifier (section 4.4): `CHAIN_RPC_<id>`, `ANVIL_URL`,
plus `RUST_LOG` / `EnvFilter` for tracing (default level INFO,
`main.rs:main`).

### 6.2 Health check

`apps/mls_validation_service/src/health_check.rs:health_check_server` — a `warp` server
bound to `0.0.0.0:<health_check_port>` with a single route `/health` returning the body
`ok` with HTTP 200. It shuts down gracefully on SIGINT/SIGTERM.

**The route has no method guard.** The filter is `warp::path("health").map(...)` with no
`warp::get()` in the chain, so `POST`, `PUT`, `DELETE` and every other method also return
`200 "ok"`. The handler is a pure constant with no side effects, so the practical risk is
low, but probes that assume method restriction will not behave as expected.

**The health check can report healthy after the gRPC server has died.** `main.rs` runs
`let _ = tokio::join!(health_server, grpc_server)`. `join!` waits for both futures and the
tuple is bound to `_`, so the gRPC server's `Result` is **discarded unexamined**. If
`serve_with_shutdown` fails — port in use, bind denied, transport setup error — nothing is
logged, the process keeps running on the health future alone, `/health` keeps answering
`200 "ok"` while gRPC is down, and `main` finally returns `Ok(())` so the **process exits
0** on a hard failure. Orchestrators keying on exit code or on this endpoint will not
notice.

Note also that `warp`'s `bind` panics rather than returning a `Result`, so a health-port
conflict aborts the process before the gRPC server is even built.

So it is a liveness check only, and a weak one: it does not test the gRPC server, the
chain RPC, or the verifier configuration. The new crate should tie readiness to the gRPC
listener and surface the serve error.

### 6.3 Docker

- `dev/validation_service/Dockerfile` — the CI/release image. `rust:1-bullseye` builder
  running `cargo build --release --features test-utils --package mls_validation_service`,
  then `debian:bullseye-slim` with `sqlite3` and `curl`, `RUST_LOG=info`, entrypoint
  `mls-validation-service`. Note it builds **with `--features test-utils`**.
- `dev/validation_service/local.Dockerfile` — local dev. `ubuntu:24.04`, copies a
  prebuilt binary from `.cache/mls-validation-service`, and `patchelf`s the interpreter
  and rpath so a Nix-built binary runs in the image.
- `dev/docker/docker-compose.yml` — service `validation`, image
  `ghcr.io/xmtp/mls-validation-service:main`, built from `local.Dockerfile`, with
  `ANVIL_URL: "http://anvil:8545"`, on the `proxynet` network. The node service is
  pointed at it with `--mls-validation.grpc-address=validation:50051`. No health check
  is declared in compose.

### 6.4 Nix

- `nix/package/mls_validation_service.nix` — a crane `buildPackage` producing
  `mls-validation-service`. It declares the minimal source fileset needed: workspace
  `Cargo.toml`/`Cargo.lock`/`.cargo/config.toml`, the non-cargo build inputs
  (`crates/xmtp_id/src/scw_verifier/chain_urls_default.json`, `crates/xmtp_id/artifact`,
  `crates/xmtp_id/src/scw_verifier/signature_validation.hex`,
  `crates/xmtp_proto/src/gen/proto_descriptor.bin`), and the crate sources for
  `xmtp-workspace-hack`, `xmtp_common`, `xmtp_logging`, `xmtp_configuration`,
  `xmtp_cryptography`, `xmtp_id`, `xmtp_proto`, `xmtp_macro`, and the app itself.
  **That list is the exact dependency closure of the validation logic** and is the best
  starting point for scoping the new crate. Musl targets get
  `RUSTFLAGS = "-C target-feature=+crt-static"`. `doCheck = false`.
- `nix/musl-docker.nix` — builds `mls-validation-service` for each cross target and wraps
  it with `dockerTools.buildLayeredImage` into `validation-service-image` (x86_64, amd64
  architecture) and `validation-service-image-aarch64-unknown-linux-musl`, both tagged
  `ghcr.io/xmtp/mls-validation-service:main` and containing `pkgs.cacert`.

### 6.5 Release

`.github/workflows/release-mls-validation-service.yml` builds
`ghcr.io/xmtp/mls-validation-service` from `dev/validation_service/Dockerfile`. On push to
`main` it tags `type=sha`; on `workflow_dispatch` it tags the workspace version, `sha`,
and `latest`; on PRs it builds without pushing.

`apps/mls_validation_service/build.rs` emits `VERGEN_GIT_SHA`. If the env var
`NIX_GIT_SHA` is set it short-circuits vergen and uses that value; otherwise it runs
`vergen_gix` with build and git (branch + sha) instructions.

---

## 7. Tests

Service:

- `apps/mls_validation_service/src/handlers.rs` (`mod tests`) —
  `test_get_association_state` (marked `#[should_panic]`; the comment says it will panic
  until signature recovery is added to the mock), `test_validate_inbox_id_key_package_happy_path`,
  `test_validate_inbox_id_key_package_failure` (pins the error string),
  `test_validate_scw` (needs a docker anvil smart wallet),
  `deserialization_error_maps_to_invalid_argument`,
  `retryable_signature_error_maps_to_unavailable`.
- `apps/mls_validation_service/src/cached_signature_verifier.rs` (`mod tests`) —
  `test_is_valid_signature` (docker smart wallet), `test_cache_eviction`,
  `test_cache_key_includes_all_params`, `test_missing_verifier`.

Validation logic:

- `crates/xmtp_id/src/key_package/verified_key_package_v2.rs` — key package parsing.
- `crates/xmtp_id/src/key_package/mls_ext_wrapper_encryption.rs` (`mod tests`).
- `crates/xmtp_id/src/associations/mod.rs` (`mod tests`) — the state-machine suite
  (`test_create_inbox`, `create_and_add_separately`, `create_and_add_together`, and more).
- `crates/xmtp_id/src/associations/association_log.rs` — action rules (via `mod.rs` tests).
- `crates/xmtp_id/src/associations/verified_signature.rs` (`mod tests`) —
  `test_recoverable_ecdsa`, `test_recoverable_ecdsa_incorrect`, `test_installation_key`,
  `validate_good_key_round_trip`, `validate_malformed_key`, `test_smart_contract_wallet`.
- `crates/xmtp_id/src/associations/signature.rs` (`mod tests`) — retryability propagation
  and `to_lower_s`.
- `crates/xmtp_id/src/associations/state.rs` (`mod tests`) — `can_add_remove`, `can_diff`.
- `crates/xmtp_id/src/associations/unsigned_actions.rs` (`mod tests`) —
  `create_signatures` pins the exact signature text.
- `crates/xmtp_id/src/associations/unverified.rs` (`mod tests`) — `create_identity_update`.
- `crates/xmtp_id/src/associations/serialization.rs` (`mod tests`), `builder.rs` (`mod tests`),
  `member.rs` (`mod tests`).
- `crates/xmtp_id/src/scw_verifier/chain_rpc_verifier.rs` (`mod tests`) — Coinbase smart
  wallet ERC-6492 cases against a docker anvil; not compiled for wasm32.
- Test fixtures: `crates/xmtp_id/src/utils/test.rs` (`docker_smart_wallet`,
  `SmartWalletContext`, `SignatureWithNonce`) and
  `crates/xmtp_id/src/associations/test_utils.rs` (`MockSmartContractSignatureVerifier`,
  `WalletTestExt`).

### 7.1 Coverage gaps at the service boundary

Every handler test lives in `apps/mls_validation_service/src/handlers.rs`, and the list
above is the whole set — six test functions. Several behaviors this page documents have
**no service-level test** and are established by reading the implementation only:

- **`ValidateGroupMessages` has no service test at all.** Nothing covers the happy path,
  the proposal-counts-as-commit quirk, trailing bytes surviving `tls_deserialize`, or
  per-item error reporting in a mixed batch.
- **Key-package tests do not cover the checks that matter.** The two key-package tests
  pin one happy path and one error string. Nothing exercises lifetime expiry or the
  missing max-range check, the outer KeyPackage signature, ciphersuite behavior,
  capability or extension handling, or credential-format and `inbox_id` behavior.
- **The `Conversion` status mapping is untested.** The two mapping tests cover
  `Deserialization` and `Signature`. They do not cover `Conversion` — which section 2.3
  shows is the variant `get_association_state` actually produces.
- **Cache staleness is untested.** `test_cache_key_includes_all_params` covers key
  distinctness and `test_cache_eviction` covers LRU capacity, but nothing covers repeated
  `None` block-number calls returning a stale verdict (section 4.6). `test_cache_eviction`
  also exercises a bare `LruCache` it builds itself, not the wrapper, so it does not test
  the caching path.
- `test_get_association_state` is `#[should_panic]`, so the main association path has no
  positive service-level assertion; its coverage comes from the `xmtp_id` unit tests.

The new crate should add tests for these before or during the port, since a port is
exactly when untested behavior drifts.

---

## 8. Summary for the crate implementer

### The whole validation surface, in one list

1. **Key package validation** — parse, verify with OpenMLS at `Mls10`, extract
   installation key and `inbox_id`. Pure CPU. The OpenMLS checks, in order, are: leaf node
   source and leaf signature, protocol version, init key ≠ encryption key, **outer
   KeyPackage signature**, key-package extensions supported by leaf capabilities, then
   `not_before <= now < not_after`. No ciphersuite allow-list, no leaf-local capability
   checks, no lifetime max-range check, no `inbox_id` format check.
   `crates/xmtp_id/src/key_package/verified_key_package_v2.rs:VerifiedKeyPackageV2::from_bytes`.
2. **Group message validation** — TLS-parse only; return hex group id and a commit-or-proposal
   flag. Pure CPU, no crypto.
   `apps/mls_validation_service/src/handlers.rs:validate_group_message`.
3. **Identity association** — verify every signature, then run the state machine over the
   old log plus the new updates; return state and diff. The only path that touches the
   network, and only for `Erc1271` signatures.
   `crates/xmtp_id/src/associations/`.
4. **Smart contract wallet verification** — a deployless ERC-6492 `eth_call` pinned to a
   block, routed by CAIP-2 chain id, with an LRU cache.
   `crates/xmtp_id/src/scw_verifier/`.

There is **no** commit-log validation and **no** `ValidateKeyPackages` RPC today.

### Five things that matter most

1. **The validation logic already lives in reusable crates.** The service binary is a thin
   shell: `handlers.rs` is 595 lines including tests, and the real work is in `xmtp_id`.
   `nix/package/mls_validation_service.nix` lists the exact dependency closure —
   `xmtp_common`, `xmtp_logging`, `xmtp_configuration`, `xmtp_cryptography`, `xmtp_id`,
   `xmtp_proto`, `xmtp_macro` — which is the right scope for the new crate. Turning this
   into a crate is mostly deleting the tonic layer.

2. **`ValidateGroupMessages` parses only — be exact about what `group_id` proves.**
   node-go derives the group id solely from the validation service's parse of the
   ciphertext and uses it as the DB routing key and dedup input
   (`/Users/nickmolnar/code/xmtp/xmtp-node-go/pkg/mls/api/v1/service.go:Service.SendGroupMessages`),
   and the client never supplies it as a separate field. Any reimplementation must keep
   that. But the service **cannot verify sender membership** — it has no group state and
   no key material — so **any client can publish to any group id** by choosing the bytes
   it serializes. Record this as a property of MLS confidentiality, not a bug to fix: the
   payload remains unreadable and unforgeable to non-members, and the residual exposure is
   topic spam. Do not describe this RPC as an authorization check. Note also that
   `is_commit` is true for **proposals as well as commits**
   (`handlers.rs:validate_group_message`) — a quirk to preserve or change deliberately,
   because node-go persists it to a DB column.

3. **Error classification is done by substring matching, and that is load-bearing.**
   xmtpd's indexer — and only xmtpd's — decides whether an identity update is permanently
   rejected or retried forever by matching **17** error strings hand-copied from libxmtp
   (`/Users/nickmolnar/code/xmtp/xmtpd/pkg/indexer/app_chain/contracts/identity_update_storer.go:shouldRetryValidationError`),
   defaulting to retry. Changing any `AssociationError` message is a breaking change for
   that backend. One pattern, `"Conversion error"`, is **already dead**: no
   `ConversionError` Display contains it and `AssociationError::Convert` is transparent, so
   terminal proto errors fall through to an unbounded 100 ms retry loop. The in-tree crate
   has the chance to fix this properly (libxmtp#3130) by returning typed errors —
   `xmtp_common::ErrorCode` labels already exist and are already attached as the
   `error-code` gRPC metadata header
   (`handlers.rs:impl From<GrpcServerError> for Status`).

4. **Retryable vs terminal is the one distinction that must survive.** The retryable set
   is exactly `VerifierError::{Provider, Io, NoVerifier, Other}` (the last delegating to
   its inner error) → `SignatureError::VerifierError` → `Unavailable`; everything else →
   `InvalidArgument` (`scw_verifier/mod.rs:impl RetryableError for VerifierError`,
   `associations/signature.rs:impl RetryableError for SignatureError`,
   `handlers.rs:impl RetryableError for GrpcServerError`). This is **not** "only chain-RPC
   trouble": `NoVerifier` is a local routing and configuration failure — an unsupported
   chain — yet it is retried forever and surfaces as a transient outage rather than a
   config error. Getting this wrong breaks welcome-sync cursor advancement for SCW users
   (xmtp/libxmtp#3394) and turns xmtpd's indexer into a retry loop.

5. **Batch semantics differ per RPC and callers depend on the details.** Key packages,
   group messages, and SCW signatures return **per-element** results and never fail the
   whole call; `GetAssociationState` is **all-or-nothing** and is the only RPC that
   returns a gRPC error. Results must be positional and the response length must equal the
   request length — node-go's `SendGroupMessages` and `UploadKeyPackage` index into the
   response without checking. Also settle the request-size limit explicitly. Today it is
   **4 MiB for both callers**: tonic's generated server decodes at a 4 MiB default that
   `main.rs` never overrides, and xmtpd's 25 MiB client call option cannot raise a
   server-side limit. Response encoding, by contrast, defaults to `usize::MAX`.

### Smaller notes worth carrying forward

- `is_inbox_id_credential` in the request is read by both Go clients but **ignored** by
  the handler.
- `expiration` in the key-package response is hard-coded to `0`; the lifetime is still
  verified inside OpenMLS but not reported. Both backends fetch it and neither uses it.
- Both backends fetch `Credential` from the key-package response and discard it
  (node-go explicitly sets `Credential: nil`).
- `client_timestamp_ns` is inside the signed text, so it is tamper-evident, but the
  backend never bounds it against wall-clock time and never requires it to increase. Note
  the signed text renders it at **second** granularity, so sub-second precision is not
  covered by the signature.
- Replay protection lives in `AssociationState::seen_signatures`, so it only works if the
  caller supplies the complete prior update log as `old_updates` — and signatures are
  recorded only after a whole update is applied, so reuse **within** one update is not
  detected.
- The SCW error strings in `VerifySmartContractWalletSignatures` responses are `{:?}`
  Debug formatting of the error enum, not `Display`.
- The health check is liveness only: `/health` → `ok`, with **no method guard**. It does
  not test the gRPC server or the chain RPCs, and because `main` discards the gRPC
  `Result` it keeps returning `ok` after the gRPC server has failed — with the process
  still exiting 0.
- The key-package path applies **no ciphersuite allow-list**, does not run OpenMLS's
  leaf-local capability checks, does not bound the lifetime range, and does not validate
  the `inbox_id` string in any way.
- Ethereum canonicalization is **lowercasing**, and it does not happen on the service's
  proto decode path at all — the supplied text is stored verbatim.
- `from_passkey` checks the challenge and the P-256 signature. It does not check
  `clientData.type`, origin, rpIdHash, flags, or the sign counter, and `relying_party` is
  excluded from passkey equality, hashing, and the signed text.
- `join_all` in the key-package and group-message handlers gives **failure isolation, not
  parallelism** — the workers have no await point, so the batch runs sequentially on the
  reactor thread.

---

## Review status

This page was checked by an adversarial review, thread id
`01a06249-4cf0-7552-a512-f2aefa74ab61` (run record:
`/Users/nickmolnar/.claude/jobs/55a23e1f/tmp/phase0/runs/review-wiki-validation.md`). The
review raised 22 findings: 5 blocker, 13 major, 4 minor. Each was verified against the
source before the text was changed. **All 22 were confirmed correct and applied.** None
was rejected.

| Finding | Applied or rejected | Note |
| --- | --- | --- |
| §2.1 key-package step list omits the outer KeyPackage signature (blocker) | applied | §2.1 now gives the eight OpenMLS checks in order. The outer signature is step 6, after the leaf signature (step 2) and after the protocol-version and init-key checks. |
| §2.1 ciphersuite and capability claims too broad (major) | applied | There is no ciphersuite comparison at all — a leaf node has no ciphersuite field — and no XMTP allow-list. `validate_locally` is not called, so leaf-local extension and credential-type capability checks do not run. |
| §2.1 lifetime max-range check omitted (major) | applied | Only `not_before <= now < not_after` is enforced. `Lifetime::has_acceptable_range` has no caller in either repo. |
| §2.1 `inbox_id` has no format validation (major) | applied | Stated in §2.1 steps and Notes. `MlsCredential` is a bare `string inbox_id`; the decoded value is never inspected. |
| §2.1 key packages are not processed in parallel (minor) | applied | `join_all` gives failure isolation only; the worker has no await point, so the batch is sequential on the task thread. |
| §5.1/§5.3/§8 `group_id` parse does not prevent cross-group writes (blocker) | applied | New subsection in §2.2, plus rewrites in §5.1, §5.3 and §8 item 2. Recorded as an inherent property of MLS confidentiality, not a defect. |
| §3.3 Ethereum canonicalization is lowercasing, not EIP-55, and the decode path preserves text (blocker) | applied | §3.3 rewritten. `sanitize_evm_addresses` lowercases; `Identifier::from_proto` and the `MemberIdentifierProto` conversion neither sanitize nor validate. |
| §3.2/§3.4 signature text incomplete: revoke target line, installation context, legacy footer (major) | applied | Revoke rows split by kind with their `(ID:/Address:/Passkey: …)` lines; context is `b"IDENTITY UPDATE SIGNATURE"`; the legacy footer's trailing `/` is contrasted with the modern footer. |
| §3.5 legacy nonce-0 rule is conditional (major) | applied | AddAssociation step 5 now explains the `Option<Identifier>` guard that skips the check for installation signers, and the `\|\|` gap it leaves. |
| §3.5 replay ordering and the raw-bytes claim (major) | applied | Signatures are recorded only after the whole update is applied, so intra-update reuse is undetected. A per-kind table shows which `raw_bytes` are canonicalized. |
| §3.4 passkey WebAuthn checks absent (major) | applied | `clientData.type`, origin, rpIdHash, flags and sign counter are all listed as unchecked; `relying_party` is shown to be inert in equality, hashing and signed text. |
| §2.3 `Conversion` vs `Deserialization`, missing enum variants (major) | applied | `try_map_vec` yields `ConversionError`, so the identity-update path produces `Conversion`. `Ed25519`, `Bincode` and `AddressValidation` added; the enum has 15 variants. |
| §2.4 `account_id` is not a message (minor) | applied | Corrected to a CAIP-10 `string`. |
| §4.3/§4.6 block-number and cache semantics (blocker) | applied | `None` resolves head only on a cache miss. The key uses the input block number, there is no TTL, and repeated `None` calls return stale wallet state until eviction. |
| §4.4 `upgrade` does not verify the `eip155:` namespace (minor) | applied | It only takes the second colon-separated component; `MalformedEipUrl` fires only when no colon is present. |
| §8 retryable set is wider than chain-RPC failures (major) | applied | Corrected in §4.5 and §8 item 4 to `Provider`, `Io`, `NoVerifier`, `Other`. `NoVerifier` is a local config failure. |
| §5.1 node-go does not convert upstream errors to `codes.Unknown` (major) | applied | A5 and A6 return the error bare, so `GRPCStatus()` is honored and codes are preserved. A3 is the contrast that does flatten. |
| §5.2 xmtpd does take an advisory lock (major) | applied | `pg_advisory_xact_lock` via `LockIdentityUpdateInsert`, taken before the state read, deliberately paired with READ COMMITTED. |
| §5.2/§8 17 patterns, and the stale `"Conversion error"` pattern (major) | applied | All 17 listed. The conversion pattern matches nothing, so terminal proto errors default to the unbounded retry loop. Only xmtpd does this matching. |
| §5/§8 request-size limits (major) | applied | The effective request limit is 4 MiB for both callers. A client call option cannot raise the tonic server limit. Response encoding defaults to `usize::MAX`. |
| §5.1/§5.2 retry details (minor) | applied | node-go makes 3 total attempts with a 0–19 ms random sleep; xmtpd retries forever at a fixed 100 ms with no backoff. Neither sets an RPC deadline. |
| §6.2 health-check behavior (major) | applied | No method guard on the route, and `let _ = tokio::join!` discards the gRPC result, so the endpoint stays healthy and the process exits 0 after a gRPC failure. |
| §7 test-coverage gaps (major) | applied | New §7.1 lists the missing group-message, key-package, `Conversion`-mapping and cache-staleness coverage. |

### Residual risk

**No tests were run.** Every correction on this page was verified by reading source, not
by executing the code, so the risk profile differs by section. Verified directly against
the implementation, with file and symbol confirmed: the OpenMLS key-package check order
and the three gaps in it (`key_package_in.rs`, `lifetime.rs`, `leaf_node.rs`); the
group-message parse; the `GrpcServerError` variants, their retryability and the
`try_map_vec` error type; the `DeserializationError` and `ConversionError` variant lists;
the address sanitizing and proto decode paths; the signature text and its action lines;
the installation-key context and legacy transcript; the state-machine step orders for all
four actions; the replay ordering; the per-constructor `raw_bytes`; the passkey checks and
the `Passkey` equality impls; the SCW cache key, its absent TTL and `upgrade`'s namespace
handling; the health-check route and `main`'s `join!`; and, on the Go side, the bare error
returns, the retry loops, the advisory lock, the 17 patterns and the client size options.

What that leaves open. Behavior asserted from reading control flow rather than observed:
the claim that a stale `None`-keyed cache entry survives an on-chain owner rotation
(the mechanism is clear from the code, but no test exercises it); the claim that a
`ConversionError` reaches xmtpd's classifier as a non-matching string end to end, which
crosses a process boundary this review did not run; and the exact effective request-size
behavior at 4 MiB, which was read from the generated tonic defaults rather than measured.
The §7.1 coverage gaps compound this: the service has no test for group-message
validation, for the `Conversion` status mapping, or for cache staleness, so those three
areas rest on inspection alone. Treat them as the first candidates for tests when the
crate is built, and re-verify the OpenMLS check order against whatever revision the new
crate pins, since it comes from a pinned `xmtp/openmls` fork rather than upstream.
