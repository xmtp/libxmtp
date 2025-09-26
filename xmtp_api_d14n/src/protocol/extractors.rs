//! Extractors transform [`ProtocolEnvelope`]'s into logical types usable by xmtp_mls

use super::{EnvelopeCollection, EnvelopeError, Extractor, ProtocolEnvelope};
use xmtp_common::{RetryableError, retryable};

mod aggregate;
pub use aggregate::*;
mod group_messages;
pub use group_messages::*;
mod identity_updates;
pub use identity_updates::*;
mod key_packages;
pub use key_packages::*;
mod payloads;
pub use payloads::*;
mod welcomes;
pub use welcomes::*;
mod topics;
pub use topics::*;

#[derive(thiserror::Error, Debug)]
pub enum ExtractionError {
    #[error(transparent)]
    Payload(#[from] PayloadExtractionError),
    #[error(transparent)]
    Topic(#[from] TopicExtractionError),
    #[error(transparent)]
    Conversion(#[from] xmtp_proto::ConversionError),
}

impl RetryableError for ExtractionError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Payload(p) => retryable!(p),
            Self::Topic(t) => retryable!(t),
            Self::Conversion(c) => retryable!(c),
        }
    }
}
