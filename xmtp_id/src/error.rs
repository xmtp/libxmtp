use openmls_traits::types::CryptoError;
use thiserror::Error;
use xmtp_mls::credential::AssociationError;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("bad association: {0}")]
    BadAssocation(#[from] AssociationError),
    #[error("generating key-pairs: {0}")]
    KeyGenerationError(#[from] CryptoError),
    #[error("uninitialized identity")]
    UninitializedIdentity,
    #[error("protobuf deserialization: {0}")]
    Deserialization(#[from] prost::DecodeError),
}
