# Error code glossary

This document catalogs every machine-readable error code produced by LibXMTP.

Error codes give developers building with XMTP SDKs a stable string they can match on, regardless of whether the human-readable message changes between releases.

## Overview

Every error surfaced through an SDK binding is formatted as:

```
[TypeName::VariantName] human-readable message
```

For example:

```
[GroupError::GroupInactive] Group is inactive
```

The code portion (`GroupError::GroupInactive`) is the **stable identifier** you should match on. The human-readable message may change between releases.

### Error inheritance

Some error variants wrap an inner error and **inherit** its code. For example, `ClientError::Storage(StorageError::NotFound(...))` produces the code `StorageError::NotFound`, not `ClientError::Storage`. This means you always see the most specific (leaf) error code, regardless of how many wrapper layers exist internally.

## Parsing errors by platform

### Kotlin / Swift (mobile)

The mobile SDK provides a built-in `parseXmtpError` function:

```kotlin
// Kotlin
try {
    client.conversations.newGroup(listOf(address))
} catch (e: GenericException) {
    val errorInfo = parseXmtpError(e.message ?: "")
    when (errorInfo.code) {
        "GroupError::GroupInactive" -> { /* handle inactive group */ }
        "StorageError::NotFound"   -> { /* handle not found */ }
        else                       -> { /* unknown error */ }
    }
}
```

```swift
// Swift
do {
    try await client.conversations.newGroup(with: [address])
} catch let error as GenericError {
    let errorInfo = parseXmtpError(message: "\(error)")
    switch errorInfo.code {
    case "GroupError::GroupInactive":
        // handle inactive group
    case "StorageError::NotFound":
        // handle not found
    default:
        break
    }
}
```

### TypeScript (Node.js)

```typescript
try {
  await client.conversations.newGroup([address]);
} catch (e: any) {
  // Error message format: "[ErrorType::Variant] message"
  const match = e.message?.match(/^\[([^\]]+)\]/);
  const code = match?.[1] ?? "Unknown";
  switch (code) {
    case "GroupError::GroupInactive":
      // handle inactive group
      break;
    case "StorageError::NotFound":
      // handle not found
      break;
  }
}
```

### JavaScript (WASM)

```javascript
try {
  await client.conversations.newGroup([address]);
} catch (e) {
  // The error object has a `code` property set directly
  const code = e.code ?? "Unknown";
  // Or parse from message: "[ErrorType::Variant] message"
  const match = e.message?.match(/^\[([^\]]+)\]/);
  const codeFromMsg = match?.[1] ?? "Unknown";
}
```

## Client errors

### `ClientError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `ClientError::PublishError` | Could not publish | Failed to publish messages to the network | Depends |
| `ClientError::Storage` | Storage error | Database operation failed | No |
| `ClientError::Api` | API error | Network request to XMTP backend failed | Yes |
| `ClientError::Identity` | Identity error | Problem with identity operations | No |
| `ClientError::TlsError` | TLS Codec error | Encoding/decoding MLS TLS structures failed | No |
| `ClientError::KeyPackageVerification` | Key package verification failed | Invalid key package received from network | No |
| `ClientError::StreamInconsistency` | Stream inconsistency | Message stream state became inconsistent | No |
| `ClientError::Association` | Association error | Identity association operation failed | No |
| `ClientError::SignatureValidation` | Signature validation error | A signature failed verification | No |
| `ClientError::IdentityUpdate` | Identity update error | Failed to process identity update | No |
| `ClientError::SignatureRequest` | Signature request error | Failed to create/process signature request | No |
| `ClientError::Group` | Group error | Group operation failed | Depends |
| `ClientError::LocalEvent` | Local event error | Failed to process local event | No |
| `ClientError::Db` | Database connection error | Connection to database failed | Yes |
| `ClientError::Generic` | Generic error | Unclassified error | Depends |
| `ClientError::MlsStore` | MLS store error | OpenMLS key store operation failed | No |
| `ClientError::EnrichMessage` | Message enrichment error | Failed to enrich message content | No |

> Some `ClientError` variants inherit the code of their inner error. For instance, a `ClientError` wrapping an `IdentifierValidationError` will surface the `IdentifierValidationError::*` code directly.

### `ClientBuilderError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `ClientBuilderError::MissingParameter` | Missing parameter | Required builder parameter not provided | No |
| `ClientBuilderError::ClientError` | Client error | Client operation failed during build | Depends |
| `ClientBuilderError::StorageError` | Storage error | Storage initialization failed | No |
| `ClientBuilderError::Identity` | Identity error | Identity creation/loading failed | No |
| `ClientBuilderError::WrappedApiError` | API error | API client initialization failed | Yes |
| `ClientBuilderError::GroupError` | Group error | Group operation failed during build | No |
| `ClientBuilderError::DeviceSync` | Device sync error | Device sync setup failed | No |
| `ClientBuilderError::OfflineBuildFailed` | Offline build failed | Builder tried to access the network in offline mode | No |

## Group / conversation errors

### `GroupError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `GroupError::UserLimitExceeded` | Max user limit exceeded | Attempted to add too many members | No |
| `GroupError::MissingSequenceId` | Sequence ID not found | Missing sequence ID in local database | No |
| `GroupError::AddressNotFound` | Addresses not found | Specified addresses have no XMTP identity | No |
| `GroupError::WrappedApi` | API error | Network request failed | Yes |
| `GroupError::InvalidGroupMembership` | Invalid group membership | Group membership state is invalid | No |
| `GroupError::LeaveCantProcessed` | Leave cannot be processed | Group leave validation failed | No |
| `GroupError::Storage` | Storage error | Database operation failed | No |
| `GroupError::Intent` | Intent error | Failed to process group intent | No |
| `GroupError::CreateMessage` | Create message error | MLS message creation failed | No |
| `GroupError::TlsError` | TLS codec error | MLS TLS encoding/decoding failed | No |
| `GroupError::UpdateGroupMembership` | Add members error | Failed to update group membership | No |
| `GroupError::GroupCreate` | Group create error | MLS group creation failed | No |
| `GroupError::SelfUpdate` | Self update error | MLS self-update operation failed | No |
| `GroupError::WelcomeError` | Welcome error | Processing MLS welcome message failed | No |
| `GroupError::InvalidExtension` | Invalid extension | MLS extension validation failed | No |
| `GroupError::Signature` | Invalid signature | MLS signature verification failed | No |
| `GroupError::Client` | Client error | Client operation failed within group | Depends |
| `GroupError::ReceiveError` | Receive error | Processing received group message failed | Depends |
| `GroupError::ReceiveErrors` | Receive errors | Multiple message processing failures | Depends |
| `GroupError::AddressValidation` | Address validation error | An address/identifier is invalid | No |
| `GroupError::LocalEvent` | Local event error | Failed to process local event | No |
| `GroupError::InvalidPublicKeys` | Invalid public keys | Keys are not valid Ed25519 public keys | No |
| `GroupError::CommitValidation` | Commit validation error | MLS commit validation failed | No |
| `GroupError::Identity` | Identity error | Identity operation failed | No |
| `GroupError::ConversionError` | Conversion error | Proto conversion failed | No |
| `GroupError::CryptoError` | Crypto error | Cryptographic operation failed | No |
| `GroupError::CreateGroupContextExtProposalError` | Group context proposal error | Failed to create group context extension proposal | No |
| `GroupError::CredentialError` | Credential error | MLS credential validation failed | No |
| `GroupError::LeafNodeError` | Leaf node error | MLS leaf node operation failed | No |
| `GroupError::InstallationDiff` | Installation diff error | Installation diff computation failed | No |
| `GroupError::NoPSKSupport` | No PSK support | Pre-shared keys are not supported | No |
| `GroupError::SqlKeyStore` | SQL key store error | OpenMLS key store operation failed | No |
| `GroupError::SyncFailedToWait` | Sync failed to wait | Waiting for intent sync failed | Yes |
| `GroupError::MissingPendingCommit` | Missing pending commit | Expected pending commit not found | No |
| `GroupError::ProcessIntent` | Process intent error | Failed to process group intent | Depends |
| `GroupError::LockUnavailable` | Failed to load lock | Concurrency lock acquisition failed | Yes |
| `GroupError::TooManyCharacters` | Exceeded max characters | Field value exceeds character limit | No |
| `GroupError::GroupPausedUntilUpdate` | Group paused until update | Group is paused until a newer version is available | No |
| `GroupError::GroupInactive` | Group is inactive | Operation on an inactive group | No |
| `GroupError::Sync` | Sync summary | Sync operation completed with errors | Depends |
| `GroupError::Db` | Database connection error | Database connection failed | Yes |
| `GroupError::MlsStore` | MLS store error | OpenMLS key store failed | No |
| `GroupError::MetadataPermissionsError` | Metadata permissions error | Metadata permission check failed | No |
| `GroupError::FailedToVerifyInstallations` | Failed to verify installations | Installation verification failed | No |
| `GroupError::NoWelcomesToSend` | No welcomes to send | No welcome messages to send to new members | No |
| `GroupError::CodecError` | Codec error | Content type codec failed | No |
| `GroupError::WrapWelcome` | Wrap welcome error | Failed to wrap welcome message | No |
| `GroupError::UnwrapWelcome` | Unwrap welcome error | Failed to unwrap welcome message | No |
| `GroupError::WelcomeDataNotFound` | Welcome data not found | Welcome data missing from topic | No |
| `GroupError::UninitializedResult` | Result not initialized | Expected result was not initialized | No |
| `GroupError::Diesel` | Diesel ORM error | Raw database query failed | No |
| `GroupError::UninitializedField` | Uninitialized field | Builder field not initialized | No |
| `GroupError::DeleteMessage` | Delete message error | Failed to delete message | No |
| `GroupError::DeviceSync` | Device sync error | Device sync operation failed | Depends |

> A `GroupError` wrapping a `NotFound` inner error will surface the `NotFound::*` code directly (e.g. `NotFound::GroupById`).

### `ReceiveErrors`

This is a **struct** error wrapping a list of message processing errors. Its error code is always `ReceiveErrors`.

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `ReceiveErrors` | Multiple receive errors | Multiple group messages failed processing | Depends |

### `GroupMutablePermissionsError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `GroupMutablePermissionsError::Serialization` | Serialization error | Failed to encode permissions protobuf | No |
| `GroupMutablePermissionsError::Deserialization` | Deserialization error | Failed to decode permissions protobuf | No |
| `GroupMutablePermissionsError::Policy` | Policy error | Permission policy validation failed | No |
| `GroupMutablePermissionsError::InvalidConversationType` | Invalid conversation type | Wrong conversation type for this operation | No |
| `GroupMutablePermissionsError::MissingPolicies` | Missing policies | Required permission policies not present | No |
| `GroupMutablePermissionsError::MissingExtension` | Missing extension | Required MLS extension not found | No |
| `GroupMutablePermissionsError::InvalidPermissionPolicyOption` | Invalid permission policy option | Invalid permission policy configuration | No |

### `GroupMetadataError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `GroupMetadataError::Serialization` | Serialization error | Failed to encode metadata protobuf | No |
| `GroupMetadataError::Deserialization` | Deserialization error | Failed to decode metadata protobuf | No |
| `GroupMetadataError::InvalidConversationType` | Invalid conversation type | Protobuf conversation type not recognized | No |
| `GroupMetadataError::MissingExtension` | Missing extension | Immutable metadata MLS extension not found | No |
| `GroupMetadataError::InvalidDmMembers` | Invalid DM members | DM member data is invalid | No |
| `GroupMetadataError::MissingDmMember` | Missing DM member | A DM member field is not set | No |

> A `GroupMetadataError` wrapping a `ConversionError` will surface the `ConversionError::*` code directly.

## Identity & association errors

### `IdentityError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `IdentityError::CredentialSerialization` | Credential serialization error | Failed to encode MLS credential | No |
| `IdentityError::Decode` | Decode error | Protobuf decoding failed | No |
| `IdentityError::InstallationIdNotFound` | Installation not found | Installation ID missing from store | No |
| `IdentityError::BasicCredential` | Basic credential error | MLS basic credential validation failed | No |
| `IdentityError::LegacyKeyReuse` | Legacy key re-use | Attempted to reuse a legacy key | No |
| `IdentityError::UninitializedIdentity` | Uninitialized identity | Identity not yet initialized | No |
| `IdentityError::InstallationKey` | Installation key error | Problem with installation key | No |
| `IdentityError::MalformedLegacyKey` | Malformed legacy key | Legacy key format is invalid | No |
| `IdentityError::LegacySignature` | Legacy signature error | Legacy signature is invalid | No |
| `IdentityError::Crypto` | Crypto error | Cryptographic operation failed | No |
| `IdentityError::LegacyKeyMismatch` | Legacy key mismatch | Legacy key does not match address | No |
| `IdentityError::OpenMls` | OpenMLS error | OpenMLS library error | No |
| `IdentityError::KeyPackageGenerationError` | Key package generation error | Failed to generate MLS key package | No |
| `IdentityError::InboxIdMismatch` | Inbox ID mismatch | Associated InboxID does not match stored value | No |
| `IdentityError::NoAssociatedInboxId` | No associated Inbox ID | Address has no associated InboxID | No |
| `IdentityError::RequiredIdentityNotFound` | Required identity not found | Identity was not found in cache | No |
| `IdentityError::NewIdentity` | New identity creation error | Error creating a new identity | No |
| `IdentityError::Signer` | Signer error | Cryptographic signer failed | No |
| `IdentityError::TooManyInstallations` | Too many installations | InboxID has reached max installation count | No |
| `IdentityError::InvalidExtension` | Invalid extension error | MLS extension validation failed | No |
| `IdentityError::MissingPostQuantumPublicKey` | Missing PQ public key | Post-quantum public key not found | No |
| `IdentityError::Bincode` | Bincode serialization error | Binary serialization failed | No |
| `IdentityError::UninitializedField` | Uninitialized field | Builder field not initialized | No |

> Many `IdentityError` variants inherit their inner error code. You may see codes from `SignatureRequestError`, `SignatureError`, `StorageError`, `SqlKeyStoreError`, `KeyPackageVerificationError`, `AssociationError`, `ApiError`, `IdentifierValidationError`, `ConnectionError`, or `GeneratePostQuantumKeyError` surfaced through an `IdentityError`.

### `AssociationError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `AssociationError::Generic` | Generic association error | Unclassified association error | No |
| `AssociationError::MultipleCreate` | Multiple create operations | Duplicate inbox creation detected | No |
| `AssociationError::NotCreated` | XID not yet created | Operating on inbox that doesn't exist yet | No |
| `AssociationError::MemberNotAllowed` | Member not allowed | Member kind cannot add the specified kind | No |
| `AssociationError::MissingExistingMember` | Missing existing member | Expected member not found in association state | No |
| `AssociationError::LegacySignatureReuse` | Legacy signature reuse | Legacy key used with non-zero nonce | No |
| `AssociationError::NewMemberIdSignatureMismatch` | New member ID signature mismatch | Signer doesn't match new member identifier | No |
| `AssociationError::WrongInboxId` | Wrong Inbox ID | Incorrect inbox_id in association | No |
| `AssociationError::SignatureNotAllowed` | Signature not allowed | Signature type not permitted for this role | No |
| `AssociationError::Replay` | Replay detected | Replayed identity update detected | No |
| `AssociationError::MissingIdentityUpdate` | Missing identity update | Required identity update not provided | No |
| `AssociationError::ChainIdMismatch` | Chain ID mismatch | Smart contract wallet chain ID changed | No |
| `AssociationError::InvalidAccountAddress` | Invalid account address | Address is not 42-char hex starting with 0x | No |
| `AssociationError::NotIdentifier` | Not an identifier | Value is not a valid public identifier | No |

> An `AssociationError` may also surface `SignatureError::*`, `DeserializationError::*`, or `ConversionError::*` codes via inheritance.

### `SignatureError` (identity)

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `SignatureError::MalformedLegacyKey` | Malformed legacy key | Legacy key format is invalid | No |
| `SignatureError::Ed25519Error` | Ed25519 signature failed | Ed25519 signature verification failed | No |
| `SignatureError::TryFromSliceError` | Slice conversion error | Byte slice conversion failed | No |
| `SignatureError::Invalid` | Signature validation failed | Signature did not verify | No |
| `SignatureError::UrlParseError` | URL parse error | CAIP-10 account ID URL is malformed | No |
| `SignatureError::DecodeError` | Decode error | Protobuf decoding failed | No |
| `SignatureError::Signer` | Signer error | Cryptographic signer operation failed | No |
| `SignatureError::InvalidPublicKey` | Invalid public key | Public key is not valid | No |
| `SignatureError::InvalidClientData` | Invalid client data | Client data is malformed | No |
| `SignatureError::SignerError` | Alloy signer error | Ethereum signer failed | No |
| `SignatureError::Signature` | Alloy signature error | Ethereum signature parsing failed | No |

> May also surface codes from the cryptography `SignatureError`, `VerifierError`, `IdentifierValidationError`, or `AccountIdError` via inheritance.

### `AccountIdError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `AccountIdError::InvalidChainId` | Invalid chain ID | Chain ID is not a valid u64 | No |
| `AccountIdError::MissingEip155Prefix` | Missing EIP-155 prefix | Chain ID not prefixed with `eip155:` | No |

### `SignatureRequestError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `SignatureRequestError::UnknownSigner` | Unknown signer | Signer not recognized for this request | No |
| `SignatureRequestError::MissingSigner` | Missing signer | Required signature was not provided | No |
| `SignatureRequestError::BlockNumber` | Unable to get block number | Failed to fetch blockchain block number | Yes |

### `DeserializationError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `DeserializationError::MissingAction` | Missing action | Identity action not present in update | No |
| `DeserializationError::MissingUpdate` | Missing update | Identity update not present | No |
| `DeserializationError::MissingMemberIdentifier` | Missing member identifier | Member identifier field empty | No |
| `DeserializationError::Signature` | Missing signature | Signature field not present | No |
| `DeserializationError::MissingMember` | Missing member | Member field not present | No |
| `DeserializationError::Decode` | Decode error | Protobuf decoding failed | No |
| `DeserializationError::InvalidAccountId` | Invalid account ID | CAIP-10 account ID is malformed | No |
| `DeserializationError::InvalidPasskey` | Invalid passkey | Passkey data is malformed | No |
| `DeserializationError::InvalidHash` | Invalid hash | Hash must be 32 bytes | No |
| `DeserializationError::Unspecified` | Unspecified field | A required field is not set | No |
| `DeserializationError::Deprecated` | Deprecated field | A deprecated field was used | No |
| `DeserializationError::Ed25519` | Ed25519 key error | Failed to create public key from bytes | No |
| `DeserializationError::Bincode` | Unable to deserialize | Bincode deserialization failed | No |

### `KeyPackageVerificationError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `KeyPackageVerificationError::TlsError` | TLS codec error | MLS TLS encoding/decoding failed | No |
| `KeyPackageVerificationError::MlsValidation` | MLS validation error | Key package verification failed | No |
| `KeyPackageVerificationError::WrongCredentialType` | Wrong credential type | Unexpected MLS credential type | No |

## Storage / database errors

### `StorageError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `StorageError::DieselConnect` | Diesel connection error | Failed to connect to SQLite | Yes |
| `StorageError::DieselResult` | Diesel result error | Database query returned an error | Depends |
| `StorageError::MigrationError` | Migration error | Database migration failed | No |
| `StorageError::NotFound` | Not found | Requested record does not exist | No |
| `StorageError::Duplicate` | Duplicate item | Attempted to insert a duplicate record | No |
| `StorageError::OpenMlsStorage` | OpenMLS storage error | OpenMLS key store operation failed | No |
| `StorageError::IntentionalRollback` | Intentional rollback | Transaction was intentionally rolled back | No |
| `StorageError::DbDeserialize` | DB deserialization failed | Failed to deserialize data from database | No |
| `StorageError::DbSerialize` | DB serialization failed | Failed to serialize data for database | No |
| `StorageError::Builder` | Builder error | Required fields missing from stored type | No |
| `StorageError::Platform` | Platform storage error | Platform-specific storage error | Depends |
| `StorageError::Prost` | Protobuf decode error | Failed to decode protobuf from database | No |
| `StorageError::Conversion` | Conversion error | Proto conversion failed | No |
| `StorageError::Connection` | Connection error | Database connection error | Yes |
| `StorageError::InvalidHmacLength` | Invalid HMAC length | HMAC key must be 42 bytes | No |
| `StorageError::GroupIntent` | Group intent error | Group intent processing failed | Depends |

### `NotFound`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `NotFound::GroupByWelcome` | Group with welcome ID not found | No group matches the welcome ID | No |
| `NotFound::GroupById` | Group with ID not found | Group does not exist in local DB | No |
| `NotFound::InstallationTimeForGroup` | Installation time for group not found | Missing installation timestamp | No |
| `NotFound::InboxIdForAddress` | Inbox ID for address not found | Address has no associated inbox | No |
| `NotFound::MessageById` | Message ID not found | Message does not exist in local DB | No |
| `NotFound::DmByInbox` | DM by inbox ID not found | No DM conversation with this inbox | No |
| `NotFound::IntentForToPublish` | Intent for ToPublish not found | Intent with specified ID not in expected state | No |
| `NotFound::IntentForPublish` | Intent for Published not found | Intent with specified ID not in expected state | No |
| `NotFound::IntentForCommitted` | Intent for Committed not found | Intent with specified ID not in expected state | No |
| `NotFound::IntentById` | Intent by ID not found | Intent does not exist | No |
| `NotFound::RefreshStateByIdKindAndOriginator` | Refresh state not found | No refresh state matching criteria | No |
| `NotFound::CipherSalt` | Cipher salt not found | Database encryption salt missing | No |
| `NotFound::SyncGroup` | Sync group not found | No sync group for this installation | No |
| `NotFound::KeyPackageReference` | Key package reference not found | Key package handle not in store | No |
| `NotFound::MlsGroup` | MLS group not found | OpenMLS group not in local state | No |
| `NotFound::PostQuantumPrivateKey` | Post-quantum private key not found | PQ key pair not in store | No |
| `NotFound::KeyPackage` | Key package not found | Key package not in store | No |

### `DuplicateItem`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `DuplicateItem::WelcomeId` | Duplicate welcome ID | Welcome ID already exists | No |
| `DuplicateItem::CommitLogPublicKey` | Duplicate commit log public key | Commit log public key for group already exists | No |

### `ConnectionError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `ConnectionError::Database` | Database error | Diesel database query error | Depends |
| `ConnectionError::DecodeError` | Decode error | Protobuf decode failed within DB layer | No |
| `ConnectionError::DisconnectInTransaction` | Disconnect in transaction | Cannot disconnect while transaction is active | No |
| `ConnectionError::ReconnectInTransaction` | Reconnect in transaction | Cannot reconnect while transaction is active | No |
| `ConnectionError::InvalidQuery` | Invalid query | Database query is malformed | No |
| `ConnectionError::InvalidVersion` | Invalid version | DB migration version mismatch -- running a newer DB on older LibXMTP | No |

> A `ConnectionError` may also surface `PlatformStorageError::*` codes via inheritance.

### `SqlKeyStoreError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `SqlKeyStoreError::UnsupportedValueTypeBytes` | Unsupported value type | Key store does not allow storing serialized values | No |
| `SqlKeyStoreError::UnsupportedMethod` | Unsupported method | Update operation not supported by this key store | No |
| `SqlKeyStoreError::SerializationError` | Serialization error | Failed to serialize value for key store | No |
| `SqlKeyStoreError::NotFound` | Value not found | Requested key not in OpenMLS key store | No |
| `SqlKeyStoreError::Storage` | Database error | Underlying Diesel database error | Depends |
| `SqlKeyStoreError::Connection` | Connection error | Database connection error | Yes |

### `GroupIntentError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `GroupIntentError::MoreThanOneDependency` | More than one dependency | Intent has multiple dependencies in same epoch | Yes |
| `GroupIntentError::NoDependencyFound` | No dependency found | Intent has no known dependencies | Yes |

### `PlatformStorageError` (native)

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `PlatformStorageError::Pool` | Pool error | Database connection pool error | Yes |
| `PlatformStorageError::DbConnection` | DB connection error | R2D2 connection manager error | Yes |
| `PlatformStorageError::PoolNeedsConnection` | Pool needs connection | Pool must reconnect before use | Yes |
| `PlatformStorageError::PoolRequiresPath` | Pool requires path | DB pool requires a persistent file path | No |
| `PlatformStorageError::SqlCipherNotLoaded` | SQLCipher not loaded | Encryption key given but SQLCipher not available | No |
| `PlatformStorageError::SqlCipherKeyIncorrect` | SQLCipher key incorrect | PRAGMA key or salt has wrong value | No |
| `PlatformStorageError::DatabaseLocked` | Database locked | Database file is locked by another process | Yes |
| `PlatformStorageError::DieselResult` | Diesel result error | Database query error | Depends |
| `PlatformStorageError::NotFound` | Not found | Record not found in storage | No |
| `PlatformStorageError::Io` | I/O error | File system I/O error | Depends |
| `PlatformStorageError::FromHex` | Hex decode error | Failed to decode hex string | No |
| `PlatformStorageError::DieselConnect` | Diesel connection error | Failed to establish connection | Yes |
| `PlatformStorageError::Boxed` | Boxed error | Wrapped dynamic error | Depends |

### `PlatformStorageError` (WASM)

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `PlatformStorageError::SAH` | OPFS error | Origin Private File System (OPFS) error | Yes |
| `PlatformStorageError::Connection` | Connection error | Diesel connection error | Yes |
| `PlatformStorageError::DieselResult` | Diesel result error | Database query error | Yes |

## API & network errors

### `ApiError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `ApiError::Api` | API client error | Network request to XMTP backend failed | Yes |
| `ApiError::MismatchedKeyPackages` | Mismatched key packages | Number of key packages doesn't match installation keys | No |
| `ApiError::ProtoConversion` | Proto conversion error | Protobuf conversion failed | No |

### `GrpcError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `GrpcError::InvalidUri` | Invalid URI | URI for channel creation is malformed | No |
| `GrpcError::Metadata` | Metadata error | Invalid gRPC metadata value | No |
| `GrpcError::Status` | gRPC status error | gRPC call returned error status | Yes |
| `GrpcError::NotFound` | Not found | Requested resource not found or empty | No |
| `GrpcError::UnexpectedPayload` | Unexpected payload | Payload not expected in response | No |
| `GrpcError::MissingPayload` | Missing payload | Expected payload not in response | No |
| `GrpcError::Decode` | Decode error | Protobuf decoding failed | No |
| `GrpcError::Unreachable` | Unreachable | Infallible error -- should never occur | No |
| `GrpcError::Transport` | Transport error | gRPC transport layer error (native only) | Yes |

> A `GrpcError` may also surface `ProtoError::*` codes via inheritance.

### `GrpcBuilderError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `GrpcBuilderError::MissingAppVersion` | Missing app version | App version not set on builder | No |
| `GrpcBuilderError::MissingLibxmtpVersion` | Missing LibXMTP version | Core library version not set | No |
| `GrpcBuilderError::MissingHostUrl` | Missing host URL | Host URL not set on builder | No |
| `GrpcBuilderError::MissingXmtpdGatewayUrl` | Missing gateway URL | xmtpd gateway URL not set | No |
| `GrpcBuilderError::Metadata` | Metadata error | Invalid gRPC metadata value | No |
| `GrpcBuilderError::InvalidUri` | Invalid URI | URI is malformed | No |
| `GrpcBuilderError::Url` | URL parse error | URL string is malformed | No |
| `GrpcBuilderError::Transport` | Transport error | gRPC transport creation failed (native only) | No |

### `MessageBackendBuilderError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `MessageBackendBuilderError::MissingV3Host` | Missing V3 host | V3 host URL not set on builder | No |
| `MessageBackendBuilderError::GrpcBuilder` | gRPC builder error | gRPC client builder failed | No |
| `MessageBackendBuilderError::MultiNode` | Multi-node error | Multi-node client builder failed | No |
| `MessageBackendBuilderError::Scw` | SCW verifier error | Smart contract wallet verifier error | No |
| `MessageBackendBuilderError::CursorStoreNotReplaced` | Cursor store not replaced | Stateful client cursor store not set | No |
| `MessageBackendBuilderError::UninitializedField` | Uninitialized field | Read/write client builder error | No |
| `MessageBackendBuilderError::ReadonlyBuilder` | Readonly builder error | Readonly client builder failed | No |
| `MessageBackendBuilderError::Builder` | Builder error | Uninitialized field in builder | No |
| `MessageBackendBuilderError::UnsupportedClient` | Unsupported client | Client kind is not supported | No |

## Device sync errors

### `DeviceSyncError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `DeviceSyncError::IO` | I/O error | File system or network I/O failed | Depends |
| `DeviceSyncError::Serde` | Serialization error | JSON serialization/deserialization failed | No |
| `DeviceSyncError::AesGcm` | AES-GCM encryption error | Encryption/decryption of sync payload failed | No |
| `DeviceSyncError::Reqwest` | HTTP request error | HTTP request for sync payload failed | Yes |
| `DeviceSyncError::Conversion` | Type conversion error | Internal type conversion failed | No |
| `DeviceSyncError::UTF8` | UTF-8 error | String is not valid UTF-8 | No |
| `DeviceSyncError::NoPendingRequest` | No pending request | No pending sync request to reply to | No |
| `DeviceSyncError::InvalidPayload` | Invalid payload | Sync message payload is malformed | No |
| `DeviceSyncError::UnspecifiedDeviceSyncKind` | Unspecified sync kind | Device sync kind not specified | No |
| `DeviceSyncError::SyncPayloadTooOld` | Sync payload too old | Sync reply is outdated | No |
| `DeviceSyncError::Bincode` | Bincode error | Binary serialization failed | No |
| `DeviceSyncError::Archive` | Archive error | Sync archive operation failed | No |
| `DeviceSyncError::Decode` | Decode error | Protobuf decoding failed | No |
| `DeviceSyncError::AlreadyAcknowledged` | Already acknowledged | Sync interaction already acknowledged | No |
| `DeviceSyncError::MissingOptions` | Missing options | Sync request options not provided | No |
| `DeviceSyncError::MissingSyncServerUrl` | Missing sync server URL | Sync server URL not configured | No |
| `DeviceSyncError::MissingSyncGroup` | Missing sync group | Sync group not found | No |
| `DeviceSyncError::Sync` | Sync summary | Sync completed with errors | Depends |
| `DeviceSyncError::MlsStore` | MLS store error | OpenMLS key store operation failed | No |
| `DeviceSyncError::Recv` | Receive error | Channel receive failed | No |
| `DeviceSyncError::MissingField` | Missing field | Required field not present | No |
| `DeviceSyncError::MissingPayload` | Missing payload | Sync payload not found for PIN | No |

> A `DeviceSyncError` may also surface codes from `ConversionError`, `StorageError`, `ClientError`, `GroupError`, `SubscribeError`, `DeserializationError`, or `ConnectionError` via inheritance.

## Content type errors

### `CodecError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `CodecError::Encode` | Encode error | Content type encoding failed | No |
| `CodecError::Decode` | Decode error | Content type decoding failed | No |
| `CodecError::CodecNotFound` | Codec not found | No codec registered for content type | No |
| `CodecError::InvalidContentType` | Invalid content type | Content type identifier is invalid | No |

## Cryptography errors

### `SignatureError` (cryptography)

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `SignatureError::BadAddressFormat` | Bad address format | Ethereum address hex decoding failed | No |
| `SignatureError::BadSignatureFormat` | Bad signature format | Signature bytes are malformed | No |
| `SignatureError::BadSignature` | Bad signature | Signature verification failed for address | No |
| `SignatureError::Signer` | Signer error | Cryptographic signer operation failed | No |
| `SignatureError::Unknown` | Unknown error | Unclassified signature error | No |

### `IdentifierValidationError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `IdentifierValidationError::InvalidAddresses` | Invalid addresses | One or more addresses failed validation | No |
| `IdentifierValidationError::HexDecode` | Hex decode error | Address hex decoding failed | No |
| `IdentifierValidationError::Generic` | Generic validation error | Unclassified validation error | No |

### `EthereumCryptoError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `EthereumCryptoError::InvalidLength` | Invalid length | Key or data has wrong length | No |
| `EthereumCryptoError::InvalidKey` | Invalid key | Cryptographic key is invalid | No |
| `EthereumCryptoError::SignFailure` | Sign failure | Signing operation failed | No |
| `EthereumCryptoError::DecompressFailure` | Decompress failure | Public key decompression failed | No |

### `GeneratePostQuantumKeyError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `GeneratePostQuantumKeyError::Crypto` | Crypto error | PQ cryptographic operation failed | No |
| `GeneratePostQuantumKeyError::Rand` | Random generation error | Random number generation failed | No |

## Protocol errors

### `ConversionError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `ConversionError::Missing` | Missing field | Required field missing during proto conversion | No |
| `ConversionError::Unspecified` | Unspecified field | Protobuf field is unspecified | No |
| `ConversionError::Deprecated` | Deprecated field | A deprecated protobuf field was used | No |
| `ConversionError::InvalidLength` | Invalid length | Data has wrong length for conversion | No |
| `ConversionError::InvalidValue` | Invalid value | Data has unexpected value | No |
| `ConversionError::Decode` | Decode error | Protobuf decoding failed | No |
| `ConversionError::Encode` | Encode error | Protobuf encoding failed | No |
| `ConversionError::UnknownEnumValue` | Unknown enum value | Protobuf enum has unrecognized value | No |
| `ConversionError::EdSignature` | Ed25519 signature error | Ed25519 signature bytes invalid | No |
| `ConversionError::InvalidPublicKey` | Invalid public key | Public key validation failed | No |
| `ConversionError::InvalidVersion` | Invalid version | Protocol version not supported | No |
| `ConversionError::OpenMls` | OpenMLS error | OpenMLS library error | No |
| `ConversionError::Protocol` | Protocol message error | MLS protocol message error | No |
| `ConversionError::Builder` | Builder error | Builder field not initialized | No |
| `ConversionError::Slice` | Slice error | Byte slice conversion failed | No |

### `ProtoError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `ProtoError::Hex` | Hex error | Hex encoding/decoding failed | No |
| `ProtoError::Decode` | Decode error | Protobuf decoding failed | No |
| `ProtoError::Encode` | Encode error | Protobuf encoding failed | No |
| `ProtoError::OpenMls` | OpenMLS error | OpenMLS library error | No |
| `ProtoError::MlsProtocolMessage` | MLS protocol message error | MLS framing error | No |
| `ProtoError::KeyPackage` | Key package error | Key package verification failed | No |
| `ProtoError::NotFound` | Not found | Proto resource not found | No |

## Subscription errors

### `SubscribeError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `SubscribeError::Group` | Group error | Group operation failed during subscription | Depends |
| `SubscribeError::NotFound` | Not found | Subscribed resource not found | No |
| `SubscribeError::GroupMessageNotFound` | Group message not found | Expected message missing from database | No |
| `SubscribeError::ReceiveGroup` | Receive group error | Processing streamed group message failed | Depends |
| `SubscribeError::Storage` | Storage error | Database operation failed | No |
| `SubscribeError::Decode` | Decode error | Protobuf decoding failed | No |
| `SubscribeError::MessageStream` | Message stream error | Message stream failed | Yes |
| `SubscribeError::ConversationStream` | Conversation stream error | Conversation stream failed | Yes |
| `SubscribeError::ApiClient` | API client error | Network request failed | Yes |
| `SubscribeError::BoxError` | Boxed error | Wrapped dynamic error | Depends |
| `SubscribeError::Db` | Database connection error | Database connection failed | Yes |
| `SubscribeError::Conversion` | Conversion error | Proto conversion failed | No |
| `SubscribeError::Envelope` | Envelope error | Decentralized API envelope error | No |

## Verification errors

### `VerifierError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `VerifierError::UnexpectedERC6492Result` | Unexpected ERC-6492 result | Smart contract wallet signature verification returned unexpected result | No |
| `VerifierError::Provider` | Provider error | Ethereum RPC provider error | Yes |
| `VerifierError::Url` | URL parse error | Verifier URL is malformed | No |
| `VerifierError::Io` | I/O error | I/O operation failed | Depends |
| `VerifierError::Serde` | Serialization error | JSON serialization/deserialization failed | No |
| `VerifierError::MalformedEipUrl` | Malformed EIP URL | URL not preceded with `eip144:` | No |
| `VerifierError::NoVerifier` | No verifier | Verifier not configured | No |
| `VerifierError::InvalidHash` | Invalid hash | Hash has invalid length or format | No |
| `VerifierError::Other` | Other error | Unclassified verifier error | Depends |

## Message enrichment errors

### `EnrichMessageError`

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `EnrichMessageError::CodecError` | Codec decode error | Content type codec failed | No |
| `EnrichMessageError::DecodeError` | Decode error | Protobuf decoding failed | No |

> An `EnrichMessageError` may also surface `ConnectionError::*` codes via inheritance.

## Generic / FFI errors

### `GenericError` (mobile)

This is the top-level error type for mobile SDKs. Almost all variants use inheritance, meaning the actual error code you see is the leaf error code, not `GenericError::*`.

| Code | Description | Common cause | Retryable? |
|------|-------------|--------------|------------|
| `GenericError::Generic` | Generic error | Unclassified error with string message | Depends |
| `GenericError::FailedToConvertToU32` | Failed to convert to u32 | Numeric conversion failed | No |
| `GenericError::JoinError` | Join error | Tokio task join failed | No |
| `GenericError::IoError` | I/O error | File or network I/O failed | Depends |
| `GenericError::LogInit` | Log init error | Failed to initialize log file | No |
| `GenericError::ReloadLog` | Reload log error | Failed to reload log subscriber | No |
| `GenericError::Log` | Log error | Error initializing debug log file | No |
| `GenericError::Expired` | Timer expired | Operation timed out | Yes |

> Most errors surfaced through the mobile SDK inherit their code from inner errors. You will typically see codes like `GroupError::GroupInactive` or `StorageError::NotFound` rather than `GenericError::*`.
