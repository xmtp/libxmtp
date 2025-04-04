//! Traits to implement functionality according to
//! https://github.com/xmtp/XIPs/blob/main/XIPs/xip-49-decentralized-backend.md#33-client-to-node-protocol

use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::{GroupMessageInput, WelcomeMessageInput};
use xmtp_proto::xmtp::xmtpv4::envelopes::{
    ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
};

/// Envelope Visitor type for ergonomic handling of serialized nested envelope types
pub trait EnvelopeVisitor {
    /// Visit the OriginatorEnvelope Type
    fn visit_originator(&mut self, envelope: &OriginatorEnvelope);
    /// Visit the UnsignedOriginatorEnvelope type
    fn visit_unsigned_originator(&mut self, envelope: &UnsignedOriginatorEnvelope);
    /// Visit the Payer Envelope Type
    fn visit_payer(&mut self, envelope: &PayerEnvelope);
    /// Visit the ClientEnvelope type
    fn visit_client(&mut self, envelope: &ClientEnvelope);
    /// Visit the GroupMessageInput type
    fn visit_group_message(&mut self, message: &GroupMessageInput);
    /// Visit the WelcomeMessageInput type
    fn visit_welcome_message(&mut self, message: &WelcomeMessageInput);
    /// Visit the Upload Key Package Type
    fn visit_upload_key_package(&mut self, package: &UploadKeyPackageRequest);
    /// Visit the Identity Update Type
    fn visit_identity_update(&mut self, update: &IdentityUpdate);
}

/// An Envelope from the backend gRPC interface
pub trait ProtocolEnvelope {
    type Output;
    fn accept<V: EnvelopeVisitor>(&self, visitor: &mut V) -> Result<(), EnvelopeError>;
    fn get(&self) -> Result<Self::Output, EnvelopeError>;
}

/// An Generic Envelope
pub trait Envelope {
    fn cursor(&self) -> usize;
    fn topic(&self) -> &[u8];
}

/// Sort
pub trait Sort {
    /// Sort envelopes by timestamp in-place
    fn timestamp_sort(&mut self);
    /// Casually Sort envelopes in-place
    fn casual_sort(&mut self, topic_cursor: usize);
}

/*
impl Sort for Vec<Envelope> {
    fn timestamp_sort(&mut self) {
        todo!()
    }

    fn casual_sort(&mut self, topic_cursor: usize) {
        todo!()
    }
}
*/
