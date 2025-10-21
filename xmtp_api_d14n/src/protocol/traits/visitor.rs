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
    welcome_message_input::{V1 as WelcomeMessageV1, Version as WelcomeMessageVersion},
};
use xmtp_proto::xmtp::xmtpv4::envelopes::{
    ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
};
use xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response;

use super::EnvelopeError;

/// Envelope Visitor type for ergonomic handling of serialized nested envelope types.
///
/// The blanket implementation on `Vec<T>` enables combining an arbitrary number of visitors into one,
///
/// process = vec![ValidateMessage::new(), ExtractMessage::new()];
/// Each step is ran in sequence, and if one of the steps fail, the entire process is
/// short-circuited.
/// This has the advantage of not re-doing deserialization for each processing step.
///
// NOTE: A new type wrapping Vec<T> can be created in order to avoid short-circuiting if that is
// desired.
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
    /// Visit the GroupMessageInput type
    fn visit_group_message_version(&mut self, _m: &GroupMessageVersion) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_group_message_version");
        Ok(())
    }
    /// Visit the WelcomeMessageInput containing the welcome message version
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

    fn visit_v3_group_message(&mut self, _m: &V3GroupMessage) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_v3_group_message");
        Ok(())
    }

    fn visit_v3_welcome_message(&mut self, _m: &V3WelcomeMessage) -> Result<(), Self::Error> {
        tracing::trace!("noop_visit_v3_welcome_message");
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
