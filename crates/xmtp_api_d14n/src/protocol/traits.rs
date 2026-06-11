//! Traits to implement functionality according to
//! <https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#33-client-to-node-protocol>

use crate::protocol::GroupMessageExtractor;
use crate::protocol::SequencedExtractor;
use crate::protocol::V3GroupMessageExtractor;
use crate::protocol::V3WelcomeMessageExtractor;
use crate::protocol::WelcomeMessageExtractor;
use derive_builder::UninitializedFieldError;
use itertools::Itertools;
use xmtp_proto::types::GlobalCursor;
use xmtp_proto::types::GroupMessage;
use xmtp_proto::types::Topic;
use xmtp_proto::types::WelcomeMessage;

use super::ExtractionError;
use super::PayloadExtractor;
use super::TopicExtractor;
use xmtp_common::Retryable;
use xmtp_common::RetryableError;
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

mod full_api;
pub use full_api::*;

mod dependency_resolution;
pub use dependency_resolution::*;

mod sort;
pub use sort::*;

mod ordered_collection;
pub use ordered_collection::*;

#[derive(thiserror::Error, Debug, Retryable)]
pub enum EnvelopeError {
    #[error(transparent)]
    #[retry(inherit)]
    Conversion(#[from] ConversionError),
    #[error(transparent)]
    #[retry(inherit)]
    Extraction(#[from] ExtractionError),
    #[error("Each topic must have a payload")]
    TopicMismatch,
    #[error("Envelope not found")]
    NotFound(&'static str),
    #[error(transparent)]
    MissingBuilderField(#[from] UninitializedFieldError),
    #[error(transparent)]
    #[retry(inherit)]
    Store(#[from] CursorStoreError),
    #[error(transparent)]
    #[retry(true)]
    Decode(#[from] prost::DecodeError),
    // for extractors defined outside of this crate or
    // generic implementations like Tuples
    #[error("{0}")]
    #[retry(inherit)]
    DynError(Box<dyn RetryableError>),
}

impl EnvelopeError {
    pub fn other(self) -> Self {
        EnvelopeError::DynError(Box::new(self) as _)
    }
}
