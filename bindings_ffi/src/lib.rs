#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]
pub mod inbox_owner;
pub mod logger;
pub mod mls;
pub mod v2;

pub use crate::inbox_owner::SigningError;
pub use mls::*;
use std::error::Error;

extern crate tracing as log;

uniffi::setup_scaffolding!("xmtpv3");

#[derive(uniffi::Error, thiserror::Error, Debug)]
#[uniffi(flat_error)]
pub enum GenericError {
    #[error("Client error: {0}")]
    Client(#[from] xmtp_mls::client::ClientError),
    #[error("Client builder error: {0}")]
    ClientBuilder(#[from] xmtp_mls::builder::ClientBuilderError),
    #[error("Storage error: {0}")]
    Storage(#[from] xmtp_mls::storage::StorageError),
    #[error("Group error: {0}")]
    GroupError(#[from] xmtp_mls::groups::GroupError),
    #[error("Signature: {0}")]
    Signature(#[from] xmtp_cryptography::signature::SignatureError),
    #[error("Group metadata: {0}")]
    GroupMetadata(#[from] xmtp_mls::groups::group_metadata::GroupMetadataError),
    #[error("Group permissions: {0}")]
    GroupMutablePermissions(
        #[from] xmtp_mls::groups::group_permissions::GroupMutablePermissionsError,
    ),
    #[error("Generic {err}")]
    Generic { err: String },
    #[error(transparent)]
    SignatureRequestError(#[from] xmtp_id::associations::builder::SignatureRequestError),
    #[error(transparent)]
    Erc1271SignatureError(#[from] xmtp_id::associations::signature::SignatureError),
    #[error(transparent)]
    Verifier(#[from] xmtp_id::scw_verifier::VerifierError),
    #[error("Failed to convert to u32")]
    FailedToConvertToU32,
    #[error("Association error: {0}")]
    Association(#[from] xmtp_id::associations::AssociationError),
    #[error(transparent)]
    DeviceSync(#[from] xmtp_mls::groups::device_sync::DeviceSyncError),
    #[error(transparent)]
    Identity(#[from] xmtp_mls::identity::IdentityError),
    #[error(transparent)]
    JoinError(#[from] tokio::task::JoinError),
    #[error(transparent)]
    IoError(#[from] tokio::io::Error),
    #[error(transparent)]
    Subscription(#[from] xmtp_mls::subscriptions::SubscribeError),
    #[error(transparent)]
    ApiClientBuild(#[from] xmtp_api_grpc::GrpcBuilderError),
    #[error(transparent)]
    Grpc(#[from] xmtp_api_grpc::GrpcError),
}

#[derive(uniffi::Error, thiserror::Error, Debug)]
#[uniffi(flat_error)]
pub enum FfiSubscribeError {
    #[error("Subscribe Error {0}")]
    Subscribe(#[from] xmtp_mls::subscriptions::SubscribeError),
    #[error("Storage error: {0}")]
    Storage(#[from] xmtp_mls::storage::StorageError),
}

impl From<String> for GenericError {
    fn from(err: String) -> Self {
        Self::Generic { err }
    }
}

impl GenericError {
    pub fn from_error<T: Error>(err: T) -> Self {
        Self::Generic {
            err: stringify_error_chain(&err),
        }
    }
}

// TODO Use non-string errors across Uniffi interface
fn stringify_error_chain<T: Error>(error: &T) -> String {
    let mut result = format!("Error: {}\n", error);

    let mut source = error.source();
    while let Some(src) = source {
        result += &format!("Caused by: {}\n", src);
        source = src.source();
    }

    result
}

#[uniffi::export]
pub fn get_version_info() -> String {
    include_str!("../libxmtp-version.txt").to_string()
}

#[cfg(test)]
mod tests {
    use crate::get_version_info;

    #[test]
    pub fn test_get_version_info() {
        print!("{}", get_version_info());
    }
}
