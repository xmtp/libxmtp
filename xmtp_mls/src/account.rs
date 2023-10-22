use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;

use crate::association::AssociationError;

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("generating new account")]
    BadGeneration(#[from] SignatureError),
    #[error("bad association")]
    BadAssocation(#[from] AssociationError),
}

pub struct Account {}
