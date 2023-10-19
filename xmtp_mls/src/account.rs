use thiserror::Error;
use xmtp_cryptography::signature::SignatureError;

use crate::association::AssociationError;

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("generating new account")]
    BadGeneration(#[from] SignatureError),
    #[error("bad association")]
    BadAssocation(#[from] AssociationError),
    #[error("mutex poisoned error")]
    MutexPoisoned,
    #[error("unknown error")]
    Unknown,
}

pub struct Account {}
