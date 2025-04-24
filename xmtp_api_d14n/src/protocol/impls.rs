//! General Blanket Implementations for protocol traits

use super::{EnvelopeError, EnvelopeVisitor, ProtocolEnvelope};
use impl_trait_for_tuples::impl_for_tuples;
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::identity::associations::IdentityUpdate;
use xmtp_proto::xmtp::mls::api::v1::GroupMessageInput;
use xmtp_proto::xmtp::mls::api::v1::UploadKeyPackageRequest;
use xmtp_proto::xmtp::mls::api::v1::WelcomeMessageInput;
use xmtp_proto::xmtp::mls::api::v1::{
    group_message_input::{V1 as GroupMessageV1, Version as GroupMessageVersion},
    welcome_message_input::{V1 as WelcomeMessageV1, Version as WelcomeMessageVersion},
};
use xmtp_proto::xmtp::xmtpv4::envelopes::{
    ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
};
use xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response;

// enables combining arbitrary # of visitors into one, ext: process = (ValidateMessage::new(), ExtractMessage::new());
// Therefore not re-doing deserialization for each processing step.
// Each step is ran sequentially in order, so if one of the steps (like Validation) fails, the whole process
// short-circuits. A combinator visitor wrapping a geenric EnvelopeVisitor, ie NoFailVisitor<V> can be created in order to avoid short-circuiting and
// store the errors somewhere else
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

    fn visit_upload_key_package(&mut self, p: &UploadKeyPackageRequest) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_upload_key_package(p)?; )* );
        Ok(())
    }

    fn visit_identity_update(&mut self, u: &IdentityUpdate) -> Result<(), Self::Error> {
        for_tuples!( #( Tuple.visit_identity_update(u)?; )* );
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
}

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

    fn visit_welcome_message_v1(&mut self, message: &WelcomeMessageV1) -> Result<(), Self::Error> {
        self.iter_mut()
            .try_for_each(|t| t.visit_welcome_message_v1(message))?;
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
}

impl<'env, T> ProtocolEnvelope<'env> for Vec<T>
where
    T: ProtocolEnvelope<'env>,
{
    type Nested<'a>
        = ()
    where
        T: 'a;

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        self.iter().try_for_each(|t| t.accept(visitor))?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}
