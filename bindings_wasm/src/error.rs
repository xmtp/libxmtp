// ! Custom error handling for WASM bindings.
// !
// ! This module provides structured error types that map to JavaScript's XmtpError class,
// ! allowing JavaScript consumers to handle errors programmatically via error codes.

use wasm_bindgen::prelude::*;

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
  code: ErrorCode,
  message: String,
  kind: Option<String>,
  details: Option<ErrorDetails>,
}

impl WasmError {
  /// Creates a new WasmError with the given code and message.
  pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
    Self {
      code,
      message: message.into(),
      kind: None,
      details: None,
    }
  }

  /// Creates a WasmError from any error type with the given code.
  pub fn from_error<E: std::error::Error>(code: ErrorCode, err: E) -> Self {
    Self {
      code,
      message: err.to_string(),
      kind: None,
      details: None,
    }
  }

  /// Sets the error kind (specific variant name).
  pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
    self.kind = Some(kind.into());
    self
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
    if let Some(ref kind) = self.kind {
      write!(f, "[{}:{}] {}", self.code.as_str(), kind, self.message)
    } else {
      write!(f, "[{}] {}", self.code.as_str(), self.message)
    }
  }
}

impl std::error::Error for WasmError {}

impl From<WasmError> for JsValue {
  fn from(err: WasmError) -> JsValue {
    let kind = match &err.kind {
      Some(k) => JsValue::from_str(k),
      None => JsValue::NULL,
    };

    let details = match &err.details {
      Some(d) => serde_wasm_bindgen::to_value(d).unwrap_or(JsValue::NULL),
      None => JsValue::NULL,
    };

    XmtpError::new(err.code.as_str(), &err.message, kind, details).into()
  }
}

impl From<serde_wasm_bindgen::Error> for WasmError {
  fn from(err: serde_wasm_bindgen::Error) -> Self {
    WasmError::encoding(err.to_string()).with_kind("SerializationError")
  }
}

impl From<xmtp_mls::groups::GroupError> for WasmError {
  fn from(err: xmtp_mls::groups::GroupError) -> Self {
    use xmtp_mls::groups::GroupError;

    let (kind, details) = match &err {
      GroupError::NotFound(nf) => (
        "NotFound",
        Some(ErrorDetails::with_field("entity", nf.to_string())),
      ),
      GroupError::UserLimitExceeded => ("UserLimitExceeded", None),
      GroupError::MissingSequenceId => ("MissingSequenceId", None),
      GroupError::AddressNotFound(addrs) => (
        "AddressNotFound",
        Some(ErrorDetails::with_field(
          "addresses",
          serde_json::json!(addrs),
        )),
      ),
      GroupError::WrappedApi(_) => ("ApiError", None),
      GroupError::InvalidGroupMembership => ("InvalidGroupMembership", None),
      GroupError::LeaveCantProcessed(leave_err) => {
        let leave_kind = match leave_err {
          xmtp_mls::groups::GroupLeaveValidationError::DmLeaveForbidden => "DmLeaveForbidden",
          xmtp_mls::groups::GroupLeaveValidationError::SingleMemberLeaveRejected => {
            "SingleMemberLeaveRejected"
          }
          xmtp_mls::groups::GroupLeaveValidationError::SuperAdminLeaveForbidden => {
            "SuperAdminLeaveForbidden"
          }
          xmtp_mls::groups::GroupLeaveValidationError::InboxAlreadyInPendingList => {
            "InboxAlreadyInPendingList"
          }
          xmtp_mls::groups::GroupLeaveValidationError::InboxNotInPendingList => {
            "InboxNotInPendingList"
          }
          xmtp_mls::groups::GroupLeaveValidationError::NotAGroupMember => "NotAGroupMember",
        };
        (
          "LeaveValidationError",
          Some(ErrorDetails::with_field("reason", leave_kind)),
        )
      }
      GroupError::Storage(_) => ("StorageError", None),
      GroupError::Intent(_) => ("IntentError", None),
      GroupError::CreateMessage(_) => ("CreateMessageError", None),
      GroupError::TlsError(_) => ("TlsCodecError", None),
      GroupError::UpdateGroupMembership(_) => ("UpdateGroupMembershipError", None),
      GroupError::GroupCreate(_) => ("GroupCreateError", None),
      GroupError::SelfUpdate(_) => ("SelfUpdateError", None),
      GroupError::WelcomeError(_) => ("WelcomeError", None),
      GroupError::InvalidExtension(_) => ("InvalidExtension", None),
      GroupError::Signature(_) => ("SignatureError", None),
      GroupError::Client(_) => ("ClientError", None),
      GroupError::ReceiveError(_) => ("ReceiveError", None),
      GroupError::ReceiveErrors(_) => ("ReceiveErrors", None),
      GroupError::AddressValidation(_) => ("AddressValidationError", None),
      GroupError::LocalEvent(_) => ("LocalEventError", None),
      GroupError::InvalidPublicKeys(keys) => (
        "InvalidPublicKeys",
        Some(ErrorDetails::with_field("count", keys.len())),
      ),
      GroupError::CommitValidation(_) => ("CommitValidationError", None),
      GroupError::Identity(_) => ("IdentityError", None),
      GroupError::ConversionError(_) => ("ConversionError", None),
      GroupError::CryptoError(_) => ("CryptoError", None),
      GroupError::CreateGroupContextExtProposalError(_) => {
        ("CreateGroupContextExtProposalError", None)
      }
      GroupError::CredentialError(_) => ("CredentialError", None),
      GroupError::LeafNodeError(_) => ("LeafNodeError", None),
      GroupError::InstallationDiff(_) => ("InstallationDiffError", None),
      GroupError::NoPSKSupport => ("NoPSKSupport", None),
      GroupError::SqlKeyStore(_) => ("SqlKeyStoreError", None),
      GroupError::SyncFailedToWait(_) => ("SyncFailedToWait", None),
      GroupError::MissingPendingCommit => ("MissingPendingCommit", None),
      GroupError::ProcessIntent(_) => ("ProcessIntentError", None),
      GroupError::LockUnavailable => ("LockUnavailable", None),
      GroupError::TooManyCharacters { length } => (
        "TooManyCharacters",
        Some(ErrorDetails::with_field("maxLength", *length)),
      ),
      GroupError::GroupPausedUntilUpdate(version) => (
        "GroupPausedUntilUpdate",
        Some(ErrorDetails::with_field("requiredVersion", version.clone())),
      ),
      GroupError::GroupInactive => ("GroupInactive", None),
      GroupError::Sync(_) => ("SyncError", None),
      GroupError::Db(_) => ("DatabaseError", None),
      GroupError::MlsStore(_) => ("MlsStoreError", None),
      GroupError::MetadataPermissionsError(_) => ("MetadataPermissionsError", None),
      GroupError::FailedToVerifyInstallations => ("FailedToVerifyInstallations", None),
      GroupError::NoWelcomesToSend => ("NoWelcomesToSend", None),
      GroupError::CodecError(_) => ("CodecError", None),
      GroupError::WrapWelcome(_) => ("WrapWelcomeError", None),
      GroupError::UnwrapWelcome(_) => ("UnwrapWelcomeError", None),
      GroupError::WelcomeDataNotFound(topic) => (
        "WelcomeDataNotFound",
        Some(ErrorDetails::with_field("topic", topic.clone())),
      ),
      GroupError::UninitializedResult => ("UninitializedResult", None),
      GroupError::Diesel(_) => ("DieselError", None),
      GroupError::UninitializedField(_) => ("UninitializedField", None),
      GroupError::EnrichMessage(_) => ("EnrichMessageError", None),
    };

    let mut wasm_err = WasmError::from_error(ErrorCode::Conversation, err).with_kind(kind);
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

    WasmError::from_error(ErrorCode::Client, err).with_kind(kind)
  }
}

impl From<xmtp_api::ApiError> for WasmError {
  fn from(err: xmtp_api::ApiError) -> Self {
    use xmtp_api::ApiError;

    let (kind, details) = match &err {
      ApiError::Api(_) => ("ApiError", None),
      ApiError::MismatchedKeyPackages {
        key_packages,
        installation_keys,
      } => (
        "MismatchedKeyPackages",
        Some(
          ErrorDetails::with_field("keyPackages", *key_packages)
            .add_field("installationKeys", *installation_keys),
        ),
      ),
      ApiError::ProtoConversion(_) => ("ProtoConversion", None),
    };

    let mut wasm_err = WasmError::from_error(ErrorCode::Api, err).with_kind(kind);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_db::StorageError> for WasmError {
  fn from(err: xmtp_db::StorageError) -> Self {
    use xmtp_db::StorageError;

    let (kind, details) = match &err {
      StorageError::DieselConnect(_) => ("DieselConnect", None),
      StorageError::DieselResult(_) => ("DieselResult", None),
      StorageError::MigrationError(_) => ("MigrationError", None),
      StorageError::NotFound(nf) => (
        "NotFound",
        Some(ErrorDetails::with_field("entity", nf.to_string())),
      ),
      StorageError::Duplicate(dup) => (
        "Duplicate",
        Some(ErrorDetails::with_field("entity", dup.to_string())),
      ),
      StorageError::OpenMlsStorage(_) => ("OpenMlsStorage", None),
      StorageError::IntentionalRollback => ("IntentionalRollback", None),
      StorageError::DbDeserialize => ("DbDeserialize", None),
      StorageError::DbSerialize => ("DbSerialize", None),
      StorageError::Builder(_) => ("Builder", None),
      StorageError::Platform(_) => ("Platform", None),
      StorageError::Prost(_) => ("Prost", None),
      StorageError::Conversion(_) => ("Conversion", None),
      StorageError::Connection(_) => ("Connection", None),
      StorageError::InvalidHmacLength => ("InvalidHmacLength", None),
    };

    let mut wasm_err = WasmError::from_error(ErrorCode::Database, err).with_kind(kind);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_mls::subscriptions::SubscribeError> for WasmError {
  fn from(err: xmtp_mls::subscriptions::SubscribeError) -> Self {
    use xmtp_mls::subscriptions::SubscribeError;

    let (kind, details) = match &err {
      SubscribeError::Group(_) => ("GroupError", None),
      SubscribeError::NotFound(nf) => (
        "NotFound",
        Some(ErrorDetails::with_field("entity", nf.to_string())),
      ),
      SubscribeError::GroupMessageNotFound => ("GroupMessageNotFound", None),
      SubscribeError::ReceiveGroup(_) => ("ReceiveGroup", None),
      SubscribeError::Storage(_) => ("StorageError", None),
      SubscribeError::Decode(_) => ("DecodeError", None),
      SubscribeError::MessageStream(_) => ("MessageStream", None),
      SubscribeError::ConversationStream(_) => ("ConversationStream", None),
      SubscribeError::ApiClient(_) => ("ApiClient", None),
      SubscribeError::BoxError(_) => ("BoxError", None),
      SubscribeError::Db(_) => ("DatabaseError", None),
      SubscribeError::Conversion(_) => ("ConversionError", None),
      SubscribeError::Envelope(_) => ("EnvelopeError", None),
      SubscribeError::MismatchedOriginators { expected, got } => (
        "MismatchedOriginators",
        Some(ErrorDetails::with_field("expected", *expected).add_field("got", *got)),
      ),
    };

    let mut wasm_err = WasmError::from_error(ErrorCode::Stream, err).with_kind(kind);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_mls::identity::IdentityError> for WasmError {
  fn from(err: xmtp_mls::identity::IdentityError) -> Self {
    use xmtp_mls::identity::IdentityError;

    let (kind, details) = match &err {
      IdentityError::CredentialSerialization(_) => ("CredentialSerialization", None),
      IdentityError::Decode(_) => ("Decode", None),
      IdentityError::InstallationIdNotFound(id) => (
        "InstallationIdNotFound",
        Some(ErrorDetails::with_field("installationId", id.clone())),
      ),
      IdentityError::SignatureRequestBuilder(_) => ("SignatureRequestBuilder", None),
      IdentityError::Signature(_) => ("Signature", None),
      IdentityError::BasicCredential(_) => ("BasicCredential", None),
      IdentityError::LegacyKeyReuse => ("LegacyKeyReuse", None),
      IdentityError::UninitializedIdentity => ("UninitializedIdentity", None),
      IdentityError::InstallationKey(_) => ("InstallationKey", None),
      IdentityError::MalformedLegacyKey(_) => ("MalformedLegacyKey", None),
      IdentityError::LegacySignature(_) => ("LegacySignature", None),
      IdentityError::Crypto(_) => ("Crypto", None),
      IdentityError::LegacyKeyMismatch => ("LegacyKeyMismatch", None),
      IdentityError::OpenMls(_) => ("OpenMls", None),
      IdentityError::StorageError(_) => ("StorageError", None),
      IdentityError::OpenMlsStorageError(_) => ("OpenMlsStorageError", None),
      IdentityError::KeyPackageGenerationError(_) => ("KeyPackageGenerationError", None),
      IdentityError::KeyPackageVerificationError(_) => ("KeyPackageVerificationError", None),
      IdentityError::InboxIdMismatch { id, stored } => (
        "InboxIdMismatch",
        Some(ErrorDetails::with_field("id", id.clone()).add_field("stored", stored.clone())),
      ),
      IdentityError::NoAssociatedInboxId(addr) => (
        "NoAssociatedInboxId",
        Some(ErrorDetails::with_field("address", addr.clone())),
      ),
      IdentityError::RequiredIdentityNotFound => ("RequiredIdentityNotFound", None),
      IdentityError::NewIdentity(_) => ("NewIdentity", None),
      IdentityError::Association(_) => ("Association", None),
      IdentityError::Signer(_) => ("Signer", None),
      IdentityError::ApiClient(_) => ("ApiClient", None),
      IdentityError::AddressValidation(_) => ("AddressValidation", None),
      IdentityError::Db(_) => ("DatabaseError", None),
      IdentityError::TooManyInstallations {
        inbox_id,
        count,
        max,
      } => (
        "TooManyInstallations",
        Some(
          ErrorDetails::with_field("inboxId", inbox_id.clone())
            .add_field("count", *count)
            .add_field("max", *max),
        ),
      ),
      IdentityError::GeneratePostQuantumKey(_) => ("GeneratePostQuantumKey", None),
      IdentityError::InvalidExtension(_) => ("InvalidExtension", None),
      IdentityError::MissingPostQuantumPublicKey => ("MissingPostQuantumPublicKey", None),
      IdentityError::Bincode => ("Bincode", None),
      IdentityError::UninitializedField(_) => ("UninitializedField", None),
    };

    let mut wasm_err = WasmError::from_error(ErrorCode::Identity, err).with_kind(kind);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<xmtp_content_types::CodecError> for WasmError {
  fn from(err: xmtp_content_types::CodecError) -> Self {
    use xmtp_content_types::CodecError;

    let (kind, details) = match &err {
      CodecError::Encode(msg) => (
        "Encode",
        Some(ErrorDetails::with_field("message", msg.clone())),
      ),
      CodecError::Decode(msg) => (
        "Decode",
        Some(ErrorDetails::with_field("message", msg.clone())),
      ),
      CodecError::CodecNotFound(content_type_id) => (
        "CodecNotFound",
        Some(ErrorDetails::with_field(
          "contentType",
          format!("{:?}", content_type_id),
        )),
      ),
      CodecError::InvalidContentType => ("InvalidContentType", None),
    };

    let mut wasm_err = WasmError::from_error(ErrorCode::ContentType, err).with_kind(kind);
    if let Some(d) = details {
      wasm_err = wasm_err.with_details(d);
    }
    wasm_err
  }
}

impl From<hex::FromHexError> for WasmError {
  fn from(err: hex::FromHexError) -> Self {
    WasmError::encoding(err.to_string()).with_kind("HexDecode")
  }
}

impl From<prost::EncodeError> for WasmError {
  fn from(err: prost::EncodeError) -> Self {
    WasmError::encoding(err.to_string()).with_kind("ProtobufEncode")
  }
}

impl From<prost::DecodeError> for WasmError {
  fn from(err: prost::DecodeError) -> Self {
    WasmError::encoding(err.to_string()).with_kind("ProtobufDecode")
  }
}

impl From<serde_json::Error> for WasmError {
  fn from(err: serde_json::Error) -> Self {
    WasmError::encoding(err.to_string()).with_kind("JsonError")
  }
}

impl From<xmtp_common::BoxDynError> for WasmError {
  fn from(err: xmtp_common::BoxDynError) -> Self {
    WasmError::unknown(err.to_string()).with_kind("BoxedError")
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
  fn test_wasm_error_display_with_kind() {
    let err = WasmError::conversation("group not found").with_kind("NotFound");
    assert_eq!(
      err.to_string(),
      "[ConversationError:NotFound] group not found"
    );
  }

  #[test]
  fn test_error_details() {
    let details = ErrorDetails::with_field("count", 5).add_field("name", "test");
    assert!(!details.is_empty());
    assert_eq!(details.fields.len(), 2);
  }
}
