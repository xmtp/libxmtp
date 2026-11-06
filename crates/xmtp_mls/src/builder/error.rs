//! Client builder errors.

use super::*;

#[derive(Error, Debug, ErrorCode)]
pub enum ClientBuilderError {
    /// The deployment storage path could not be resolved or opened.
    /// May be retryable if local storage becomes available.
    #[error(transparent)]
    #[error_code("StorageLocation")]
    StorageLocation(#[from] crate::storage_location::StorageLocationError),
    /// Attachment storage could not be prepared or cleaned.
    /// May be retryable if local storage becomes available.
    #[error(transparent)]
    #[error_code("Attachment")]
    Attachment(#[from] crate::attachments::AttachmentClientError),
    #[error(transparent)]
    #[error_code(inherit)]
    AddressValidation(#[from] IdentifierValidationError),
    /// Missing parameter.
    ///
    /// Required builder parameter not provided. Not retryable.
    #[error("Missing parameter: {parameter}")]
    MissingParameter { parameter: &'static str },
    /// Client error.
    ///
    /// Client operation failed during build. May be retryable.
    #[error(transparent)]
    ClientError(#[from] crate::client::ClientError),
    /// Storage error.
    ///
    /// Storage initialization failed. Not retryable.
    #[error("Storage Error")]
    StorageError(#[from] StorageError),
    /// Identity error.
    ///
    /// Identity creation/loading failed. Not retryable.
    #[error(transparent)]
    Identity(#[from] crate::identity::IdentityError),
    /// API error.
    ///
    /// API client initialization failed. Retryable.
    #[error(transparent)]
    WrappedApiError(#[from] xmtp_api::ApiError),
    /// Group error.
    ///
    /// Group operation failed during build. Not retryable.
    #[error(transparent)]
    GroupError(#[from] Box<crate::groups::GroupError>),
    /// Device sync error.
    ///
    /// Device sync setup failed. Not retryable.
    #[error(transparent)]
    DeviceSync(#[from] Box<crate::worker::device_sync::DeviceSyncError>),
    /// Offline build failed.
    ///
    /// Builder tried to access the network in offline mode. Not retryable.
    #[error("Offline build failed, builder tried to access the network")]
    OfflineBuildFailed,
}

impl From<crate::worker::device_sync::DeviceSyncError> for ClientBuilderError {
    fn from(value: crate::worker::device_sync::DeviceSyncError) -> Self {
        ClientBuilderError::DeviceSync(Box::new(value))
    }
}

impl From<crate::groups::GroupError> for ClientBuilderError {
    fn from(value: crate::groups::GroupError) -> Self {
        ClientBuilderError::GroupError(Box::new(value))
    }
}
