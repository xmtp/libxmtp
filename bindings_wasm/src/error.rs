// ! Custom error handling for WASM bindings.
// !
// ! This module provides structured error types that map to JavaScript's XmtpError class,
// ! allowing JavaScript consumers to handle errors programmatically via error codes.

use wasm_bindgen::prelude::*;
use xmtp_mls::error_details::ErrorDetailsProvider;

// Import the custom XmtpError class from JavaScript
#[wasm_bindgen(module = "/src/error.js")]
extern "C" {
  #[wasm_bindgen(extends = js_sys::Error)]
  pub type XmtpError;

  #[wasm_bindgen(constructor)]
  fn new(code: &str, message: &str, kind: JsValue, details: JsValue) -> XmtpError;
}

/// Error codes for categorizing errors in JavaScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
  /// Client initialization or configuration errors
  Client,
  /// Database operation errors
  Database,
  /// Cryptographic or signature errors
  Signature,
  /// Message encoding/decoding errors
  Encoding,
  /// API or network errors
  Api,
  /// Permission or authorization errors
  Permission,
  /// Conversation operation errors
  Conversation,
  /// Stream subscription errors
  Stream,
  /// Identity or inbox errors
  Identity,
  /// Content type errors
  ContentType,
  /// Generic/unknown errors
  Unknown,
}

impl ErrorCode {
  /// Returns the string code for this error category.
  pub fn as_str(&self) -> &'static str {
    match self {
      ErrorCode::Client => "ClientError",
      ErrorCode::Database => "DatabaseError",
      ErrorCode::Signature => "SignatureError",
      ErrorCode::Encoding => "EncodingError",
      ErrorCode::Api => "ApiError",
      ErrorCode::Permission => "PermissionError",
      ErrorCode::Conversation => "ConversationError",
      ErrorCode::Stream => "StreamError",
      ErrorCode::Identity => "IdentityError",
      ErrorCode::ContentType => "ContentTypeError",
      ErrorCode::Unknown => "UnknownError",
    }
  }
}

impl From<ErrorCode> for String {
  fn from(value: ErrorCode) -> Self {
    value.as_str().to_string()
  }
}

/// Optional structured details about an error.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ErrorDetails {
  /// Additional context fields
  #[serde(flatten)]
  pub fields: std::collections::HashMap<String, serde_json::Value>,
}

impl ErrorDetails {
  /// Creates empty details.
  pub fn empty() -> Self {
    Self::default()
  }

  /// Creates details from a serde_json map.
  pub fn from_map(map: serde_json::Map<String, serde_json::Value>) -> Self {
    Self {
      fields: map.into_iter().collect(),
    }
  }

  /// Creates details with a single field.
  pub fn with_field(key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
    let mut fields = std::collections::HashMap::new();
    fields.insert(key.into(), value.into());
    Self { fields }
  }

  /// Adds a field to the details.
  pub fn add_field(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
    self.fields.insert(key.into(), value.into());
    self
  }

  /// Returns true if there are no details.
  pub fn is_empty(&self) -> bool {
    self.fields.is_empty()
  }
}

/// A wrapper for creating structured XMTP errors.
///
/// This struct wraps any error type and associates it with an error code,
/// producing JavaScript XmtpError instances with both a code and message.
#[derive(Debug)]
pub struct WasmError {
  code: String,
  message: String,
  details: Option<ErrorDetails>,
}

impl WasmError {
  /// Creates a new WasmError with the given code and message.
  pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
    Self {
      code: code.into(),
      message: message.into(),
      details: None,
    }
  }

  /// Creates a WasmError from any error type with the given code.
  pub fn from_error<E: std::error::Error>(code: impl Into<String>, err: E) -> Self {
    Self {
      code: code.into(),
      message: err.to_string(),
      details: None,
    }
  }

  /// Sets the error details.
  pub fn with_details(mut self, details: ErrorDetails) -> Self {
    if !details.is_empty() {
      self.details = Some(details);
    }
    self
  }

  /// Creates a client error.
  pub fn client(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Client, message)
  }

  /// Creates a database error.
  pub fn database(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Database, message)
  }

  /// Creates a signature error.
  pub fn signature(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Signature, message)
  }

  /// Creates an encoding error.
  pub fn encoding(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Encoding, message)
  }

  /// Creates an API error.
  pub fn api(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Api, message)
  }

  /// Creates a permission error.
  pub fn permission(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Permission, message)
  }

  /// Creates a conversation error.
  pub fn conversation(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Conversation, message)
  }

  /// Creates a stream error.
  pub fn stream(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Stream, message)
  }

  /// Creates an identity error.
  pub fn identity(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Identity, message)
  }

  /// Creates a content type error.
  pub fn content_type(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::ContentType, message)
  }

  /// Creates an unknown/generic error.
  pub fn unknown(message: impl Into<String>) -> Self {
    Self::new(ErrorCode::Unknown, message)
  }
}

impl std::fmt::Display for WasmError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "[{}] {}", self.code, self.message)
  }
}

impl std::error::Error for WasmError {}

impl From<WasmError> for JsValue {
  fn from(err: WasmError) -> JsValue {
    let details = match &err.details {
      Some(d) => serde_wasm_bindgen::to_value(d).unwrap_or(JsValue::NULL),
      None => JsValue::NULL,
    };

    XmtpError::new(&err.code, &err.message, JsValue::NULL, details).into()
  }
}

impl From<serde_wasm_bindgen::Error> for WasmError {
  fn from(err: serde_wasm_bindgen::Error) -> Self {
    let kind = "SerializationError";
    let code = format!("EncodingError::{kind}");
    WasmError::from_error(code, err)
  }
}

impl From<xmtp_mls::groups::GroupError> for WasmError {
  fn from(err: xmtp_mls::groups::GroupError) -> Self {
    use xmtp_mls::groups::GroupError;

    let kind = match &err {
      GroupError::NotFound(_) => "NotFound",
      GroupError::UserLimitExceeded => "UserLimitExceeded",
      GroupError::MissingSequenceId => "MissingSequenceId",
      GroupError::AddressNotFound(_) => "AddressNotFound",
      GroupError::WrappedApi(_) => "ApiError",
      GroupError::InvalidGroupMembership => "InvalidGroupMembership",
      GroupError::LeaveCantProcessed(_) => "LeaveValidationError",
      GroupError::Storage(_) => "StorageError",
      GroupError::Intent(_) => "IntentError",
      GroupError::CreateMessage(_) => "CreateMessageError",
      GroupError::TlsError(_) => "TlsCodecError",
      GroupError::UpdateGroupMembership(_) => "UpdateGroupMembershipError",
      GroupError::GroupCreate(_) => "GroupCreateError",
      GroupError::SelfUpdate(_) => "SelfUpdateError",
      GroupError::WelcomeError(_) => "WelcomeError",
      GroupError::InvalidExtension(_) => "InvalidExtension",
      GroupError::Signature(_) => "SignatureError",
      GroupError::Client(_) => "ClientError",
      GroupError::ReceiveError(_) => "ReceiveError",
      GroupError::ReceiveErrors(_) => "ReceiveErrors",
      GroupError::AddressValidation(_) => "AddressValidationError",
      GroupError::LocalEvent(_) => "LocalEventError",
      GroupError::InvalidPublicKeys(_) => "InvalidPublicKeys",
      GroupError::CommitValidation(_) => "CommitValidationError",
      GroupError::Identity(_) => "IdentityError",
      GroupError::ConversionError(_) => "ConversionError",
      GroupError::CryptoError(_) => "CryptoError",
      GroupError::CreateGroupContextExtProposalError(_) => "CreateGroupContextExtProposalError",
      GroupError::CredentialError(_) => "CredentialError",
      GroupError::LeafNodeError(_) => "LeafNodeError",
      GroupError::InstallationDiff(_) => "InstallationDiffError",
      GroupError::NoPSKSupport => "NoPSKSupport",
      GroupError::SqlKeyStore(_) => "SqlKeyStoreError",
      GroupError::SyncFailedToWait(_) => "SyncFailedToWait",
      GroupError::MissingPendingCommit => "MissingPendingCommit",
      GroupError::ProcessIntent(_) => "ProcessIntentError",
      GroupError::LockUnavailable => "LockUnavailable",
      GroupError::TooManyCharacters { .. } => "TooManyCharacters",
      GroupError::GroupPausedUntilUpdate(_) => "GroupPausedUntilUpdate",
      GroupError::GroupInactive => "GroupInactive",
      GroupError::Sync(_) => "SyncError",
      GroupError::Db(_) => "DatabaseError",
      GroupError::MlsStore(_) => "MlsStoreError",
      GroupError::MetadataPermissionsError(_) => "MetadataPermissionsError",
      GroupError::FailedToVerifyInstallations => "FailedToVerifyInstallations",
      GroupError::NoWelcomesToSend => "NoWelcomesToSend",
      GroupError::CodecError(_) => "CodecError",
      GroupError::WrapWelcome(_) => "WrapWelcomeError",
      GroupError::UnwrapWelcome(_) => "UnwrapWelcomeError",
      GroupError::WelcomeDataNotFound(_) => "WelcomeDataNotFound",
      GroupError::UninitializedResult => "UninitializedResult",
      GroupError::Diesel(_) => "DieselError",
      GroupError::UninitializedField(_) => "UninitializedField",
      GroupError::EnrichMessage(_) => "EnrichMessageError",
    };

    let details = err.details().map(ErrorDetails::from_map);
    let code = format!("GroupError::{kind}");
    let mut wasm_err = WasmError::from_error(code, err);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_mls::client::ClientError> for WasmError {
  fn from(err: xmtp_mls::client::ClientError) -> Self {
    use xmtp_mls::client::ClientError;

    let kind = match &err {
      ClientError::AddressValidation(_) => "AddressValidation",
      ClientError::PublishError(_) => "PublishError",
      ClientError::Storage(_) => "StorageError",
      ClientError::Api(_) => "ApiError",
      ClientError::Identity(_) => "IdentityError",
      ClientError::TlsError(_) => "TlsCodecError",
      ClientError::KeyPackageVerification(_) => "KeyPackageVerification",
      ClientError::StreamInconsistency(_) => "StreamInconsistency",
      ClientError::Association(_) => "AssociationError",
      ClientError::SignatureValidation(_) => "SignatureValidation",
      ClientError::IdentityUpdate(_) => "IdentityUpdate",
      ClientError::SignatureRequest(_) => "SignatureRequest",
      ClientError::Group(_) => "GroupError",
      ClientError::LocalEvent(_) => "LocalEvent",
      ClientError::Db(_) => "DatabaseError",
      ClientError::Generic(_) => "Generic",
      ClientError::MlsStore(_) => "MlsStoreError",
      ClientError::EnrichMessage(_) => "EnrichMessage",
    };

    let code = format!("ClientError::{kind}");
    WasmError::from_error(code, err)
  }
}

impl From<xmtp_api::ApiError> for WasmError {
  fn from(err: xmtp_api::ApiError) -> Self {
    use xmtp_api::ApiError;

    let kind = match &err {
      ApiError::Api(_) => "ApiError",
      ApiError::MismatchedKeyPackages { .. } => "MismatchedKeyPackages",
      ApiError::ProtoConversion(_) => "ProtoConversion",
    };

    let details = err.details().map(ErrorDetails::from_map);
    let code = format!("ApiError::{kind}");
    let mut wasm_err = WasmError::from_error(code, err);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_db::StorageError> for WasmError {
  fn from(err: xmtp_db::StorageError) -> Self {
    use xmtp_db::StorageError;

    let kind = match &err {
      StorageError::DieselConnect(_) => "DieselConnect",
      StorageError::DieselResult(_) => "DieselResult",
      StorageError::MigrationError(_) => "MigrationError",
      StorageError::NotFound(_) => "NotFound",
      StorageError::Duplicate(_) => "Duplicate",
      StorageError::OpenMlsStorage(_) => "OpenMlsStorage",
      StorageError::IntentionalRollback => "IntentionalRollback",
      StorageError::DbDeserialize => "DbDeserialize",
      StorageError::DbSerialize => "DbSerialize",
      StorageError::Builder(_) => "Builder",
      StorageError::Platform(_) => "Platform",
      StorageError::Prost(_) => "Prost",
      StorageError::Conversion(_) => "Conversion",
      StorageError::Connection(_) => "Connection",
      StorageError::InvalidHmacLength => "InvalidHmacLength",
    };

    let details = err.details().map(ErrorDetails::from_map);
    let code = format!("StorageError::{kind}");
    let mut wasm_err = WasmError::from_error(code, err);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_mls::subscriptions::SubscribeError> for WasmError {
  fn from(err: xmtp_mls::subscriptions::SubscribeError) -> Self {
    use xmtp_mls::subscriptions::SubscribeError;

    let kind = match &err {
      SubscribeError::Group(_) => "GroupError",
      SubscribeError::NotFound(_) => "NotFound",
      SubscribeError::GroupMessageNotFound => "GroupMessageNotFound",
      SubscribeError::ReceiveGroup(_) => "ReceiveGroup",
      SubscribeError::Storage(_) => "StorageError",
      SubscribeError::Decode(_) => "DecodeError",
      SubscribeError::MessageStream(_) => "MessageStream",
      SubscribeError::ConversationStream(_) => "ConversationStream",
      SubscribeError::ApiClient(_) => "ApiClient",
      SubscribeError::BoxError(_) => "BoxError",
      SubscribeError::Db(_) => "DatabaseError",
      SubscribeError::Conversion(_) => "ConversionError",
      SubscribeError::Envelope(_) => "EnvelopeError",
      SubscribeError::MismatchedOriginators { .. } => "MismatchedOriginators",
    };

    let details = err.details().map(ErrorDetails::from_map);
    let code = format!("SubscribeError::{kind}");
    let mut wasm_err = WasmError::from_error(code, err);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_mls::identity::IdentityError> for WasmError {
  fn from(err: xmtp_mls::identity::IdentityError) -> Self {
    use xmtp_mls::identity::IdentityError;

    let kind = match &err {
      IdentityError::CredentialSerialization(_) => "CredentialSerialization",
      IdentityError::Decode(_) => "Decode",
      IdentityError::InstallationIdNotFound(_) => "InstallationIdNotFound",
      IdentityError::SignatureRequestBuilder(_) => "SignatureRequestBuilder",
      IdentityError::Signature(_) => "Signature",
      IdentityError::BasicCredential(_) => "BasicCredential",
      IdentityError::LegacyKeyReuse => "LegacyKeyReuse",
      IdentityError::UninitializedIdentity => "UninitializedIdentity",
      IdentityError::InstallationKey(_) => "InstallationKey",
      IdentityError::MalformedLegacyKey(_) => "MalformedLegacyKey",
      IdentityError::LegacySignature(_) => "LegacySignature",
      IdentityError::Crypto(_) => "Crypto",
      IdentityError::LegacyKeyMismatch => "LegacyKeyMismatch",
      IdentityError::OpenMls(_) => "OpenMls",
      IdentityError::StorageError(_) => "StorageError",
      IdentityError::OpenMlsStorageError(_) => "OpenMlsStorageError",
      IdentityError::KeyPackageGenerationError(_) => "KeyPackageGenerationError",
      IdentityError::KeyPackageVerificationError(_) => "KeyPackageVerificationError",
      IdentityError::InboxIdMismatch { .. } => "InboxIdMismatch",
      IdentityError::NoAssociatedInboxId(_) => "NoAssociatedInboxId",
      IdentityError::RequiredIdentityNotFound => "RequiredIdentityNotFound",
      IdentityError::NewIdentity(_) => "NewIdentity",
      IdentityError::Association(_) => "Association",
      IdentityError::Signer(_) => "Signer",
      IdentityError::ApiClient(_) => "ApiClient",
      IdentityError::AddressValidation(_) => "AddressValidation",
      IdentityError::Db(_) => "DatabaseError",
      IdentityError::TooManyInstallations { .. } => "TooManyInstallations",
      IdentityError::GeneratePostQuantumKey(_) => "GeneratePostQuantumKey",
      IdentityError::InvalidExtension(_) => "InvalidExtension",
      IdentityError::MissingPostQuantumPublicKey => "MissingPostQuantumPublicKey",
      IdentityError::Bincode => "Bincode",
      IdentityError::UninitializedField(_) => "UninitializedField",
    };

    let details = err.details().map(ErrorDetails::from_map);
    let code = format!("IdentityError::{kind}");
    let mut wasm_err = WasmError::from_error(code, err);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_content_types::CodecError> for WasmError {
  fn from(err: xmtp_content_types::CodecError) -> Self {
    use xmtp_content_types::CodecError;

    let kind = match &err {
      CodecError::Encode(_) => "Encode",
      CodecError::Decode(_) => "Decode",
      CodecError::CodecNotFound(_) => "CodecNotFound",
      CodecError::InvalidContentType => "InvalidContentType",
    };

    let details = err.details().map(ErrorDetails::from_map);
    let code = format!("CodecError::{kind}");
    let mut wasm_err = WasmError::from_error(code, err);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<hex::FromHexError> for WasmError {
  fn from(err: hex::FromHexError) -> Self {
    let kind = "HexDecode";
    let code = format!("EncodingError::{kind}");
    WasmError::from_error(code, err)
  }
}

impl From<prost::EncodeError> for WasmError {
  fn from(err: prost::EncodeError) -> Self {
    let kind = "ProtobufEncode";
    let code = format!("EncodingError::{kind}");
    WasmError::from_error(code, err)
  }
}

impl From<prost::DecodeError> for WasmError {
  fn from(err: prost::DecodeError) -> Self {
    let kind = "ProtobufDecode";
    let code = format!("EncodingError::{kind}");
    WasmError::from_error(code, err)
  }
}

impl From<serde_json::Error> for WasmError {
  fn from(err: serde_json::Error) -> Self {
    let kind = "JsonError";
    let code = format!("EncodingError::{kind}");
    WasmError::from_error(code, err)
  }
}

impl From<xmtp_common::BoxDynError> for WasmError {
  fn from(err: xmtp_common::BoxDynError) -> Self {
    let kind = "BoxedError";
    let code = format!("UnknownError::{kind}");
    WasmError::from_error(code, err)
  }
}

/// Extension trait for easily converting errors to WasmError.
pub trait IntoWasmError<T> {
  /// Converts the error to a WasmError with the given code.
  fn wasm_err(self, code: ErrorCode) -> Result<T, WasmError>;
}

impl<T, E: std::error::Error> IntoWasmError<T> for Result<T, E> {
  fn wasm_err(self, code: ErrorCode) -> Result<T, WasmError> {
    self.map_err(|e| WasmError::from_error(code, e))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_error_code_strings() {
    assert_eq!(ErrorCode::Client.as_str(), "ClientError");
    assert_eq!(ErrorCode::Database.as_str(), "DatabaseError");
    assert_eq!(ErrorCode::Unknown.as_str(), "UnknownError");
  }

  #[test]
  fn test_wasm_error_display() {
    let err = WasmError::client("test message");
    assert_eq!(err.to_string(), "[ClientError] test message");
  }

  #[test]
  fn test_error_details() {
    let details = ErrorDetails::with_field("count", 5).add_field("name", "test");
    assert!(!details.is_empty());
    assert_eq!(details.fields.len(), 2);
  }
}
