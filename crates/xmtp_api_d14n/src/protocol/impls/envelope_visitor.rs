//!  enables combining arbitrary # of visitors into one, ext: process = (ValidateMessage::new(), ExtractMessage::new());
//! Therefore not re-doing deserialization for each processing step.
//! Each step is ran sequentially in order, so if one of the steps (like Validation) fails, the whole process
//! short-circuits. A combinator visitor wrapping a generic EnvelopeVisitor, ie NoFailVisitor<V> can be created in order to avoid short-circuiting and
//! store the errors somewhere else

use crate::protocol::{EnvelopeError, EnvelopeVisitor};
use impl_trait_for_tuples::impl_for_tuples;
use xmtp_proto::identity_v1::get_identity_updates_request;
use xmtp_proto::identity_v1::get_identity_updates_response::IdentityUpdateLog;
use xmtp_proto::mls_v1::fetch_key_packages_response::KeyPackage;
use xmtp_proto::mls_v1::subscribe_group_messages_request::Filter as SubscribeGroupMessagesFilter;
use xmtp_proto::mls_v1::subscribe_welcome_messages_request::Filter as SubscribeWelcomeMessagesFilter;
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::GroupMessageInput;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessageInput;
use xmtp_proto::xmtp::mls::api::v1::{
    group_message::V1 as V3GroupMessage, welcome_message::V1 as V3WelcomeMessage,
};
use xmtp_proto::xmtp::mls::api::v1::{
    group_message_input::{V1 as GroupMessageV1, Version as GroupMessageVersion},
    welcome_message_input::{V1 as WelcomeMessageV1, Version as WelcomeMessageVersion},
};
use xmtp_proto::xmtp::xmtpv4::envelopes::{
    ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
};
use xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response;

type ForError<'a, V> = <V as EnvelopeVisitor<'a>>::Error;

#[impl_for_tuples(1, 6)]
impl<'a> EnvelopeVisitor<'a> for Tuple {
    type Error = EnvelopeError;

    for_tuples!( where #( EnvelopeError: From<ForError<'a, Tuple>> )* );

    fn visit_originator(&mut self, envelope: &OriginatorEnvelope) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_originator(envelope)?; )* );
        Ok(())
    }

    fn visit_unsigned_originator(
        &mut self,
        e: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_unsigned_originator(e)?; )* );
        Ok(())
    }

    fn visit_payer(&mut self, e: &PayerEnvelope) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_payer(e)?; )* );
        Ok(())
    }

    fn visit_client(&mut self, e: &ClientEnvelope) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_client(e)?; )* );
        Ok(())
    }

    fn visit_group_message_version(&mut self, m: &GroupMessageVersion) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_group_message_version(m)?; )* );
        Ok(())
    }

    fn visit_group_message_input(&mut self, m: &GroupMessageInput) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_group_message_input(m)?; )* );
        Ok(())
    }

    fn visit_group_message_v1(&mut self, m: &GroupMessageV1) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_group_message_v1(m)?; )* );
        Ok(())
    }

    fn visit_welcome_message_version(
        &mut self,
        m: &WelcomeMessageVersion,
    ) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_welcome_message_version(m)?; )* );
        Ok(())
    }

    fn visit_welcome_message_input(&mut self, m: &WelcomeMessageInput) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_welcome_message_input(m)?; )* );
        Ok(())
    }

    fn visit_welcome_message_v1(&mut self, m: &WelcomeMessageV1) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_welcome_message_v1(m)?; )* );
        Ok(())
    }

    fn visit_v3_group_message(&mut self, m: &V3GroupMessage) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_v3_group_message(m)?; )* );
        Ok(())
    }

    fn visit_v3_welcome_message(&mut self, m: &V3WelcomeMessage) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_v3_welcome_message(m)?; )* );
        Ok(())
    }

    fn visit_upload_key_package(&mut self, p: &UploadKeyPackageRequest) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_upload_key_package(p)?; )* );
        Ok(())
    }

    fn visit_identity_update(&mut self, u: &IdentityUpdate) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_identity_update(u)?; )* );
        Ok(())
    }

    fn visit_identity_update_log(&mut self, u: &IdentityUpdateLog) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_identity_update_log(u)?; )* );
        Ok(())
    }

    fn visit_identity_updates_request(
        &mut self,
        u: &get_identity_updates_request::Request,
    ) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_identity_updates_request(u)?; )* );
        Ok(())
    }

    fn visit_key_package(&mut self, k: &KeyPackage) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_key_package(k)?; )* );
        Ok(())
    }

    fn visit_none(&mut self) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_none()?; )* );
        Ok(())
    }

    /// Visit a Newest Envelope Response
    fn visit_newest_envelope_response(
        &mut self,
        u: &get_newest_envelope_response::Response,
    ) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_newest_envelope_response(u)?; )* );
        Ok(())
    }

    /// visit_subscribe_group_messages_request
    fn visit_subscribe_group_messages_request(
        &mut self,
        r: &SubscribeGroupMessagesFilter,
    ) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_subscribe_group_messages_request(r)?; )* );
        Ok(())
    }

    /// visit_subscribe_group_messages_request
    fn visit_subscribe_welcome_messages_request(
        &mut self,
        r: &SubscribeWelcomeMessagesFilter,
    ) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_subscribe_welcome_messages_request(r)?; )* );
        Ok(())
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn test_visit_u32(&mut self, n: &u32) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.test_visit_u32(n)?; )* );
        Ok(())
    }
}

// run extractors of the same type in sequence
impl<'a, T> EnvelopeVisitor<'a> for Vec<T>
where
    T: EnvelopeVisitor<'a>,
    T::Error: std::error::Error,
{
    type Error = T::Error;

    fn visit_originator(&mut self, envelope: &OriginatorEnvelope) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_originator(envelope))?;
        Ok(())
    }

    fn visit_unsigned_originator(
        &mut self,
        envelope: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_unsigned_originator(envelope))?;
        Ok(())
    }

    fn visit_payer(&mut self, envelope: &PayerEnvelope) -> Result<(), Self::Error> {
        self.iter_mut().try_for_each(|t| t.visit_payer(envelope))?;
        Ok(())
    }

    fn visit_client(&mut self, envelope: &ClientEnvelope) -> Result<(), Self::Error> {
        self.iter_mut().try_for_each(|t| t.visit_client(envelope))?;
        Ok(())
    }

    fn visit_group_message_version(
        &mut self,
        message: &GroupMessageVersion,
    ) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_group_message_version(message))?;
        Ok(())
    }

    fn visit_group_message_input(&mut self, m: &GroupMessageInput) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_group_message_input(m))?;
        Ok(())
    }

    fn visit_group_message_v1(&mut self, message: &GroupMessageV1) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_group_message_v1(message))?;
        Ok(())
    }

    fn visit_welcome_message_version(
        &mut self,
        message: &WelcomeMessageVersion,
    ) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_welcome_message_version(message))?;
        Ok(())
    }

    fn visit_welcome_message_input(&mut self, m: &WelcomeMessageInput) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_welcome_message_input(m))?;
        Ok(())
    }

    fn visit_welcome_message_v1(&mut self, message: &WelcomeMessageV1) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_welcome_message_v1(message))?;
        Ok(())
    }

    fn visit_v3_group_message(&mut self, m: &V3GroupMessage) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_v3_group_message(m))?;
        Ok(())
    }

    fn visit_v3_welcome_message(&mut self, m: &V3WelcomeMessage) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_v3_welcome_message(m))?;
        Ok(())
    }

    fn visit_upload_key_package(
        &mut self,
        package: &UploadKeyPackageRequest,
    ) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_upload_key_package(package))?;
        Ok(())
    }

    fn visit_identity_update(&mut self, update: &IdentityUpdate) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_identity_update(update))?;
        Ok(())
    }

    fn visit_identity_update_log(&mut self, u: &IdentityUpdateLog) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_identity_update_log(u))?;
        Ok(())
    }

    /// Visit an Identity Updates Request
    fn visit_identity_updates_request(
        &mut self,
        u: &get_identity_updates_request::Request,
    ) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_identity_updates_request(u))?;
        Ok(())
    }

    fn visit_key_package(&mut self, k: &KeyPackage) -> Result<(), Self::Error> {
        self.iter_mut().try_for_each(|t| t.visit_key_package(k))?;
        Ok(())
    }

    /// Visit an empty type in a fixed-length array
    /// Useful is client expects a constant length between
    /// requests and responses
    fn visit_none(&mut self) -> Result<(), Self::Error> {
        self.iter_mut().try_for_each(|t| t.visit_none())?;
        Ok(())
    }

    /// Visit a Newest Envelope Response
    fn visit_newest_envelope_response(
        &mut self,
        u: &get_newest_envelope_response::Response,
    ) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_newest_envelope_response(u))?;
        Ok(())
    }

    /// visit_subscribe_group_messages_request
    fn visit_subscribe_group_messages_request(
        &mut self,
        r: &SubscribeGroupMessagesFilter,
    ) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_subscribe_group_messages_request(r))?;
        Ok(())
    }

    /// visit_subscribe_group_messages_request
    fn visit_subscribe_welcome_messages_request(
        &mut self,
        r: &SubscribeWelcomeMessagesFilter,
    ) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_subscribe_welcome_messages_request(r))?;
        Ok(())
    }

    #[cfg(any(test, feature = "test-utils"))]
    fn test_visit_u32(&mut self, n: &u32) -> Result<(), Self::Error> {
        self.iter_mut().try_for_each(|t| t.test_visit_u32(n))?;
        Ok(())
    }
}
