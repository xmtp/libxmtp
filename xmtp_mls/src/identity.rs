use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;

use crate::association::AssociationError;

#[derive(Debug, Error)]
pub enum IdentityError {
    #[error("generating new identity")]
    BadGeneration(#[from] SignatureError),
    #[error("bad association")]
    BadAssocation(#[from] AssociationError),
}

pub struct Identity {}
