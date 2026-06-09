//! Extractors transform [`ProtocolEnvelope`]'s into logical types usable by xmtp_mls

use super::{EnvelopeCollection, EnvelopeError, Extractor, ProtocolEnvelope};
use xmtp_common::Retryable;

mod aggregate;
pub use aggregate::*;
mod group_messages;
pub use group_messages::*;
mod identity_updates;
pub use identity_updates::*;
mod group_message_metadata;
pub use group_message_metadata::*;
mod key_packages;
pub use key_packages::*;
mod payloads;
pub use payloads::*;
mod welcomes;
pub use welcomes::*;
mod topics;
pub use topics::*;
mod data;
pub use data::*;
mod cursor;
pub use cursor::*;
mod timestamp;
pub use timestamp::*;
mod depends_on;
pub use depends_on::*;
mod bytes;
pub use bytes::*;
mod orphaned_envelope;
pub use orphaned_envelope::*;

#[cfg(any(test, feature = "test-utils"))]
pub mod test_utils;

#[derive(thiserror::Error, Debug, Retryable)]
pub enum ExtractionError {
    #[error(transparent)]
    #[retry(inherit)]
    Payload(#[from] PayloadExtractionError),
    #[error(transparent)]
    #[retry(inherit)]
    Topic(#[from] TopicExtractionError),
    #[error(transparent)]
    #[retry(inherit)]
    Conversion(#[from] xmtp_proto::ConversionError),
}
