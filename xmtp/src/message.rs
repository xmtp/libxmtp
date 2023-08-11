use crate::{association::AssociationError, contact::ContactError};
use prost::{DecodeError, EncodeError};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("association error")]
    Association(#[from] AssociationError),
    #[error("contact error")]
    Contact(#[from] ContactError),
    #[error("bad data")]
    BadData(String),
    #[error("decode error")]
    Decode(#[from] DecodeError),
    #[error("encode error")]
    Encode(#[from] EncodeError),
    #[error("unknown error")]
    Unknown,
}

pub fn decode_bytes<T: prost::Message + Default>(bytes: &[u8]) -> Result<T, PayloadError> {
    Ok(T::decode(bytes)?)
}
