use std::any::Any;

use xmtp_api::ApiError;
use xmtp_content_types::CodecError;
use xmtp_db::StorageError;
use xmtp_mls::client::ClientError;
use xmtp_mls::groups::GroupError;
use xmtp_mls::identity::IdentityError;
use xmtp_mls::subscriptions::SubscribeError;

/// Best-effort mapping from Rust errors to a namespaced `Enum::Variant` code string.
pub fn code_for_error<E: std::error::Error + 'static>(err: &E) -> Option<String> {
  let any = err as &dyn Any;

  if let Some(e) = any.downcast_ref::<GroupError>() {
    return Some(format!("GroupError::{}", code_for_group_error(e)));
  }
  if let Some(e) = any.downcast_ref::<ClientError>() {
    return Some(format!("ClientError::{}", code_for_client_error(e)));
  }
  if let Some(e) = any.downcast_ref::<ApiError>() {
    return Some(format!("ApiError::{}", code_for_api_error(e)));
  }
  if let Some(e) = any.downcast_ref::<StorageError>() {
    return Some(format!("StorageError::{}", code_for_storage_error(e)));
  }
  if let Some(e) = any.downcast_ref::<SubscribeError>() {
    return Some(format!("SubscribeError::{}", code_for_subscribe_error(e)));
  }
  if let Some(e) = any.downcast_ref::<IdentityError>() {
    return Some(format!("IdentityError::{}", code_for_identity_error(e)));
  }
  if let Some(e) = any.downcast_ref::<CodecError>() {
    return Some(format!("CodecError::{}", code_for_codec_error(e)));
  }

  None
}

fn code_for_group_error(err: &GroupError) -> &'static str {
  use GroupError::*;
  match err {
    NotFound(_) => "NotFound",
    UserLimitExceeded => "UserLimitExceeded",
    MissingSequenceId => "MissingSequenceId",
    AddressNotFound(_) => "AddressNotFound",
    WrappedApi(_) => "ApiError",
    InvalidGroupMembership => "InvalidGroupMembership",
    LeaveCantProcessed(_) => "LeaveValidationError",
    Storage(_) => "StorageError",
    Intent(_) => "IntentError",
    CreateMessage(_) => "CreateMessageError",
    TlsError(_) => "TlsCodecError",
    UpdateGroupMembership(_) => "UpdateGroupMembershipError",
    GroupCreate(_) => "GroupCreateError",
    SelfUpdate(_) => "SelfUpdateError",
    WelcomeError(_) => "WelcomeError",
    InvalidExtension(_) => "InvalidExtension",
    Signature(_) => "SignatureError",
    Client(_) => "ClientError",
    ReceiveError(_) => "ReceiveError",
    ReceiveErrors(_) => "ReceiveErrors",
    AddressValidation(_) => "AddressValidationError",
    LocalEvent(_) => "LocalEventError",
    InvalidPublicKeys(_) => "InvalidPublicKeys",
    CommitValidation(_) => "CommitValidationError",
    Identity(_) => "IdentityError",
    ConversionError(_) => "ConversionError",
    CryptoError(_) => "CryptoError",
    CreateGroupContextExtProposalError(_) => "CreateGroupContextExtProposalError",
    CredentialError(_) => "CredentialError",
    LeafNodeError(_) => "LeafNodeError",
    InstallationDiff(_) => "InstallationDiffError",
    NoPSKSupport => "NoPSKSupport",
    SqlKeyStore(_) => "SqlKeyStoreError",
    SyncFailedToWait(_) => "SyncFailedToWait",
    MissingPendingCommit => "MissingPendingCommit",
    ProcessIntent(_) => "ProcessIntentError",
    LockUnavailable => "LockUnavailable",
    TooManyCharacters { .. } => "TooManyCharacters",
    GroupPausedUntilUpdate(_) => "GroupPausedUntilUpdate",
    GroupInactive => "GroupInactive",
    Sync(_) => "SyncError",
    Db(_) => "DatabaseError",
    MlsStore(_) => "MlsStoreError",
    MetadataPermissionsError(_) => "MetadataPermissionsError",
    FailedToVerifyInstallations => "FailedToVerifyInstallations",
    NoWelcomesToSend => "NoWelcomesToSend",
    CodecError(_) => "CodecError",
    WrapWelcome(_) => "WrapWelcomeError",
    UnwrapWelcome(_) => "UnwrapWelcomeError",
    WelcomeDataNotFound(_) => "WelcomeDataNotFound",
    UninitializedResult => "UninitializedResult",
    Diesel(_) => "DieselError",
    UninitializedField(_) => "UninitializedField",
    EnrichMessage(_) => "EnrichMessageError",
  }
}

fn code_for_client_error(err: &ClientError) -> &'static str {
  use ClientError::*;
  match err {
    AddressValidation(_) => "AddressValidation",
    PublishError(_) => "PublishError",
    Storage(_) => "StorageError",
    Api(_) => "ApiError",
    Identity(_) => "IdentityError",
    TlsError(_) => "TlsCodecError",
    KeyPackageVerification(_) => "KeyPackageVerification",
    StreamInconsistency(_) => "StreamInconsistency",
    Association(_) => "AssociationError",
    SignatureValidation(_) => "SignatureValidation",
    IdentityUpdate(_) => "IdentityUpdate",
    SignatureRequest(_) => "SignatureRequest",
    Group(_) => "GroupError",
    LocalEvent(_) => "LocalEvent",
    Db(_) => "DatabaseError",
    Generic(_) => "Generic",
    MlsStore(_) => "MlsStoreError",
    EnrichMessage(_) => "EnrichMessage",
  }
}

fn code_for_api_error(err: &ApiError) -> &'static str {
  use ApiError::*;
  match err {
    Api(_) => "ApiError",
    MismatchedKeyPackages { .. } => "MismatchedKeyPackages",
    ProtoConversion(_) => "ProtoConversion",
  }
}

fn code_for_storage_error(err: &StorageError) -> &'static str {
  use StorageError::*;
  match err {
    DieselConnect(_) => "DieselConnect",
    DieselResult(_) => "DieselResult",
    MigrationError(_) => "MigrationError",
    NotFound(_) => "NotFound",
    Duplicate(_) => "Duplicate",
    OpenMlsStorage(_) => "OpenMlsStorage",
    IntentionalRollback => "IntentionalRollback",
    DbDeserialize => "DbDeserialize",
    DbSerialize => "DbSerialize",
    Builder(_) => "Builder",
    Platform(_) => "Platform",
    Prost(_) => "Prost",
    Conversion(_) => "Conversion",
    Connection(_) => "Connection",
    InvalidHmacLength => "InvalidHmacLength",
  }
}

fn code_for_subscribe_error(err: &SubscribeError) -> &'static str {
  use SubscribeError::*;
  match err {
    Group(_) => "GroupError",
    NotFound(_) => "NotFound",
    GroupMessageNotFound => "GroupMessageNotFound",
    ReceiveGroup(_) => "ReceiveGroup",
    Storage(_) => "StorageError",
    Decode(_) => "DecodeError",
    MessageStream(_) => "MessageStream",
    ConversationStream(_) => "ConversationStream",
    ApiClient(_) => "ApiClient",
    BoxError(_) => "BoxError",
    Db(_) => "DatabaseError",
    Conversion(_) => "ConversionError",
    Envelope(_) => "EnvelopeError",
    MismatchedOriginators { .. } => "MismatchedOriginators",
  }
}

fn code_for_identity_error(err: &IdentityError) -> &'static str {
  use IdentityError::*;
  match err {
    CredentialSerialization(_) => "CredentialSerialization",
    Decode(_) => "Decode",
    InstallationIdNotFound(_) => "InstallationIdNotFound",
    SignatureRequestBuilder(_) => "SignatureRequestBuilder",
    Signature(_) => "Signature",
    BasicCredential(_) => "BasicCredential",
    LegacyKeyReuse => "LegacyKeyReuse",
    UninitializedIdentity => "UninitializedIdentity",
    InstallationKey(_) => "InstallationKey",
    MalformedLegacyKey(_) => "MalformedLegacyKey",
    LegacySignature(_) => "LegacySignature",
    Crypto(_) => "Crypto",
    LegacyKeyMismatch => "LegacyKeyMismatch",
    OpenMls(_) => "OpenMls",
    StorageError(_) => "StorageError",
    OpenMlsStorageError(_) => "OpenMlsStorageError",
    KeyPackageGenerationError(_) => "KeyPackageGenerationError",
    KeyPackageVerificationError(_) => "KeyPackageVerificationError",
    InboxIdMismatch { .. } => "InboxIdMismatch",
    NoAssociatedInboxId(_) => "NoAssociatedInboxId",
    RequiredIdentityNotFound => "RequiredIdentityNotFound",
    NewIdentity(_) => "NewIdentity",
    Association(_) => "Association",
    Signer(_) => "Signer",
    ApiClient(_) => "ApiClient",
    AddressValidation(_) => "AddressValidation",
    Db(_) => "DatabaseError",
    TooManyInstallations { .. } => "TooManyInstallations",
    GeneratePostQuantumKey(_) => "GeneratePostQuantumKey",
    InvalidExtension(_) => "InvalidExtension",
    MissingPostQuantumPublicKey => "MissingPostQuantumPublicKey",
    Bincode => "Bincode",
    UninitializedField(_) => "UninitializedField",
  }
}

fn code_for_codec_error(err: &CodecError) -> &'static str {
  use CodecError::*;
  match err {
    Encode(_) => "Encode",
    Decode(_) => "Decode",
    CodecNotFound(_) => "CodecNotFound",
    InvalidContentType => "InvalidContentType",
  }
}
