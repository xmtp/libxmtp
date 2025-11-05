//! Traits to implement functionality according to
//! <https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#33-client-to-node-protocol>

use crate::protocol::GroupMessageExtractor;
use crate::protocol::SequencedExtractor;
use crate::protocol::V3GroupMessageExtractor;
use crate::protocol::V3WelcomeMessageExtractor;
use crate::protocol::WelcomeMessageExtractor;
use itertools::Itertools;
use xmtp_proto::types::GlobalCursor;
use xmtp_proto::types::GroupMessage;
use xmtp_proto::types::Topic;
use xmtp_proto::types::WelcomeMessage;

use super::ExtractionError;
use super::PayloadExtractor;
use super::TopicExtractor;
use xmtp_common::RetryableError;
use xmtp_common::retryable;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::xmtpv4::envelopes::AuthenticatedData;
use xmtp_proto::xmtp::xmtpv4::envelopes::ClientEnvelope;
use xmtp_proto::xmtp::xmtpv4::envelopes::client_envelope::Payload;

mod visitor;
pub use visitor::*;

mod cursor_store;
pub use cursor_store::*;

mod envelopes;
pub use envelopes::*;

mod xmtp_query;
pub use xmtp_query::*;

mod extractor;
pub use extractor::*;

mod envelope_collection;
pub use envelope_collection::*;

mod vector_clock;
pub use vector_clock::*;

mod full_api;
pub use full_api::*;

#[derive(thiserror::Error, Debug)]
pub enum EnvelopeError {
    #[error(transparent)]
    Conversion(#[from] ConversionError),
    #[error(transparent)]
    Extraction(#[from] ExtractionError),
    #[error("Each topic must have a payload")]
    TopicMismatch,
    #[error("Envelope not found")]
    NotFound(&'static str),
    // for extractors defined outside of this crate or
    // generic implementations like Tuples
    #[error("{0}")]
    DynError(Box<dyn RetryableError>),
}

impl RetryableError for EnvelopeError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Conversion(c) => retryable!(c),
            Self::Extraction(e) => retryable!(e),
            Self::TopicMismatch => false,
            Self::DynError(d) => retryable!(d),
            Self::NotFound(_) => false,
        }
    }
}
