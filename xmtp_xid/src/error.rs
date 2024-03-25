use openmls::prelude::{CredentialError, InvalidExtensionError, KeyPackageNewError};
use openmls_traits::types::CryptoError;
use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;
use xmtp_mls::{credential::AssociationError, storage::StorageError};

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("generating new identity: {0}")]
    BadGeneration(#[from] SignatureError),
    #[error("bad association: {0}")]
    BadAssocation(#[from] AssociationError),
    #[error("generating key-pairs: {0}")]
    KeyGenerationError(#[from] CryptoError),
    #[error("storage error: {0}")]
    StorageError(#[from] StorageError),
    #[error("generating key package: {0}")]
    KeyPackageGenerationError(#[from] KeyPackageNewError<StorageError>),
    #[error("deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
    #[error("invalid extension: {0}")]
    InvalidExtension(#[from] InvalidExtensionError),
    #[error("uninitialized identity")]
    UninitializedIdentity,
    #[error("wallet signature required - please sign the text produced by text_to_sign()")]
    WalletSignatureRequired,
    #[error("tls serialization: {0}")]
    TlsSerialization(#[from] tls_codec::Error),
    #[error("api error: {0}")]
    ApiError(#[from] xmtp_proto::api_client::Error),
    #[error("OpenMLS credential error: {0}")]
    OpenMlsCredentialError(#[from] CredentialError),
}
