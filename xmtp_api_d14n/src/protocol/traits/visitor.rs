use xmtp_proto::identity_v1::get_identity_updates_response::IdentityUpdateLog;
use xmtp_proto::mls_v1::fetch_key_packages_response::KeyPackage;
use xmtp_proto::mls_v1::subscribe_group_messages_request::Filter as SubscribeGroupMessagesFilter;
use xmtp_proto::mls_v1::subscribe_welcome_messages_request::Filter as SubscribeWelcomeMessagesFilter;
use xmtp_proto::xmtp::identity::api::v1::get_identity_updates_request;
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::GroupMessageInput;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessageInput;
use xmtp_proto::xmtp::mls::api::v1::{
    get_newest_group_message_response,
    group_message::V1 as V3GroupMessage,
    group_message_input::{V1 as GroupMessageV1, Version as GroupMessageVersion},
    welcome_message::V1 as V3WelcomeMessage,
    welcome_message::WelcomePointer as V3WelcomePointer,
    welcome_message_input::{
        V1 as WelcomeMessageV1, Version as WelcomeMessageVersion,
        WelcomePointer as WelcomeMessageWelcomePointer,
    },
};
use xmtp_proto::xmtp::xmtpv4::envelopes::{
    ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
};
use xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response;

use super::EnvelopeError;

/// Envelope Visitor type for handling of serialized nested envelope types.
///
/// Envelope visitors allow implementors to define data needed from
/// a [`ProtocolEnvelope`](super::ProtocolEnvelope).
/// It is designed such that like-kinded types may be extracted in the same module. For instance,
/// V3 and D14n Group Messages. A visitor is given to
/// a ProtocolEnvelope implementation
/// via the [`ProtocolEnvelope::accept`](super::ProtocolEnvelope::accept) method. The implementation of
/// ProtocolEnvelope
/// defines how the protobuf data structure is traversed. it is the responsibility of
/// ProtocolEnvelope to call all relevant visitor methods defined on this trait.
/// if a visitor is not called, it must not be present in the given input data of
/// a ProtocolEnvelope
///
/// The [`Envelope`](super::Envelope) and [`EnvelopeCollection`](super::EnvelopeCollection) makes handling collections of
/// [`ProtocolEnvelope`](super::ProtocolEnvelope) and applying their extractors more convenient, as
/// it provides a blanket implementation on all [`ProtocolEnvelope`](super::ProtocolEnvelope)
/// types.
///
/// The Aggregate Extractors, [`CollectionExtractor`](crate::protocol::extractors::CollectionExtractor) and
/// [`SequencedExtractor`](crate::protocol::extractors::SequencedExtractor) applies an extractor to
/// collections (Vec::<T>) of [`ProtocolEnvelope`](super::ProtocolEnvelope)'s.
///
/// # Examples
///
///### Run a single visitor
/// ```
/// # use xmtp_proto::mls_v1;
/// # use xmtp_api_d14n::protocol::extractors::V3GroupMessageExtractor;
/// # use xmtp_api_d14n::protocol::{ProtocolEnvelope, Extractor};
/// // [`mls_v1::GroupMessage`] has a [`ProtocolEnvelope`] implementation.
/// fn get_group_message(response: mls_v1::GroupMessage) -> xmtp_proto::types::GroupMessage {
///     // our Extractor which has an implementation of [`EnvelopeVisitor`]
///     let mut visitor = V3GroupMessageExtractor::default();
///     response.accept(&mut visitor);
///     let msg = visitor.get().unwrap();
///     msg.unwrap()
/// }
/// ```
/// ## Run multiple visitors
/// Running multiple visitors is useful when you want to extract multiple things
/// from an envelope, or it's uncertain whether the envelopes contains a certain type of message.
/// Visitors chained as a tuple should always run through the protobuf message exactly once O(n).
///
/// _NOTE:_ the blanket implementation of [`Envelope`](super::Envelope) on all [`ProtocolEnvelope`](super::ProtocolEnvelope) types
/// removes much of the boilerplate here, and can have functions added if needed.
/// ```
/// # use xmtp_proto::mls_v1;
/// # use xmtp_proto::types::GroupMessage;
/// # use xmtp_api_d14n::protocol::extractors::{TopicExtractor, PayloadExtractor};
/// # use xmtp_api_d14n::protocol::{ProtocolEnvelope, Extractor};
/// # use xmtp_proto::xmtp::xmtpv4::envelopes::OriginatorEnvelope;
/// # fn get_topic_and_payload(envelope: OriginatorEnvelope) {
///     let topic = TopicExtractor::default();
///     let payload = PayloadExtractor::default();
///     let mut visitor = (topic, payload);
///     envelope.accept(&mut visitor);
///     // unwrap the visitor from its tuple
///     let (topic, payload) = visitor;
///     let topic = topic.get().unwrap();
///     let payload = payload.get().unwrap();
/// # }
/// ```
pub trait EnvelopeVisitor<'env> {
    type Error: Into<EnvelopeError>;
    /// Visit the OriginatorEnvelope Type
    fn visit_originator(&mut self, _e: &OriginatorEnvelope) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_originator");
        Ok(())
    }
    /// Visit the UnsignedOriginatorEnvelope type
    fn visit_unsigned_originator(
        &mut self,
        _e: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_unsigned_originator");
        Ok(())
    }
    /// Visit the Payer Envelope Type
    fn visit_payer(&mut self, _e: &PayerEnvelope) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_payer");
        Ok(())
    }
    /// Visit the ClientEnvelope type
    fn visit_client(&mut self, _e: &ClientEnvelope) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_client");
        Ok(())
    }
    /// Visit the GroupMessageVersion type
    fn visit_group_message_version(&mut self, _m: &GroupMessageVersion) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_group_message_version");
        Ok(())
    }
    /// Visit the GroupMessageInput containing the welcome message version
    fn visit_group_message_input(&mut self, _m: &GroupMessageInput) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_group_message_input");
        Ok(())
    }

    /// Visit a V1 Group Message
    fn visit_group_message_v1(&mut self, _m: &GroupMessageV1) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_group_message_v1");
        Ok(())
    }
    /// Visit the WelcomeMessageInput containing the welcome message version
    fn visit_welcome_message_version(
        &mut self,
        _m: &WelcomeMessageVersion,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_group_message_version");
        Ok(())
    }
    /// Visit the WelcomeMessageInput containing the welcome message version
    fn visit_welcome_message_input(&mut self, _m: &WelcomeMessageInput) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_welcome_message_input");
        Ok(())
    }

    /// Visit a V1 Welcome Message
    fn visit_welcome_message_v1(&mut self, _m: &WelcomeMessageV1) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_welcome_message_v1");
        Ok(())
    }

    /// Visit a Welcome Pointer
    fn visit_welcome_pointer(
        &mut self,
        _m: &WelcomeMessageWelcomePointer,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_welcome_pointer");
        Ok(())
    }

    fn visit_v3_group_message(&mut self, _m: &V3GroupMessage) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_v3_group_message");
        Ok(())
    }

    fn visit_v3_welcome_message(&mut self, _m: &V3WelcomeMessage) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_v3_welcome_message");
        Ok(())
    }

    fn visit_v3_welcome_pointer(&mut self, _m: &V3WelcomePointer) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_v3_welcome_pointer");
        Ok(())
    }

    /// Visit the Upload Key Package
    fn visit_upload_key_package(
        &mut self,
        _p: &UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_upload_key_package");
        Ok(())
    }

    /// Visit the Identity Update Type
    fn visit_identity_update(&mut self, _u: &IdentityUpdate) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_identity_update");
        Ok(())
    }

    fn visit_identity_update_log(&mut self, _u: &IdentityUpdateLog) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_identity_update_log");
        Ok(())
    }

    /// Visit an Identity Updates Request
    fn visit_identity_updates_request(
        &mut self,
        _u: &get_identity_updates_request::Request,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_identity_updates_request");
        Ok(())
    }

    fn visit_key_package(&mut self, _k: &KeyPackage) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_key_package");
        Ok(())
    }

    /// Visit an empty type in a fixed-length array
    /// Useful is client expects a constant length between
    /// requests and responses
    fn visit_none(&mut self) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_none");
        Ok(())
    }

    /// Visit a Newest Envelope Response
    fn visit_newest_envelope_response(
        &mut self,
        _u: &get_newest_envelope_response::Response,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_newest_envelope_response");
        Ok(())
    }

    /// visit_subscribe_group_messages_request
    fn visit_subscribe_group_messages_request(
        &mut self,
        _r: &SubscribeGroupMessagesFilter,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_subscribe_group_messages_request");
        Ok(())
    }

    /// visit_subscribe_group_messages_request
    fn visit_subscribe_welcome_messages_request(
        &mut self,
        _r: &SubscribeWelcomeMessagesFilter,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_subscribe_group_messages_request");
        Ok(())
    }

    fn visit_newest_group_message_response(
        &mut self,
        _u: &get_newest_group_message_response::Response,
    ) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_newest_group_message_response");
        Ok(())
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn test_visit_u32(&mut self, _n: &u32) -> Result<(), Self::Error> {
        tracing::trace!("noop_test_visit_u32");
        Ok(())
    }
}
