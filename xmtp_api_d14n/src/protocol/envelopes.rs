//! Implementions of traits
use super::traits::{EnvelopeError, EnvelopeVisitor, ProtocolEnvelope};
use prost::Message;
use xmtp_proto::mls_v1::{SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest};
use xmtp_proto::xmtp::xmtpv4::message_api::get_newest_envelope_response;
use xmtp_proto::{
    ConversionError,
    xmtp::identity::{api::v1::get_identity_updates_request, associations::IdentityUpdate},
    xmtp::mls::api::v1::UploadKeyPackageRequest,
    xmtp::mls::api::v1::{
        GroupMessageInput, WelcomeMessageInput,
        group_message_input::Version as GroupMessageVersion,
        welcome_message_input::Version as WelcomeMessageVersion,
    },
    xmtp::xmtpv4::envelopes::client_envelope::Payload,
    xmtp::xmtpv4::envelopes::{
        ClientEnvelope, OriginatorEnvelope, PayerEnvelope, UnsignedOriginatorEnvelope,
    },
};

impl<'env> ProtocolEnvelope<'env> for OriginatorEnvelope {
    type Nested<'a> = UnsignedOriginatorEnvelope;

    fn accept<V: super::EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_originator(self)?;
        let unsigned = self.get_nested()?;
        unsigned.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(UnsignedOriginatorEnvelope::decode(
            self.unsigned_originator_envelope.as_slice(),
        )?)
    }
}

impl<'env> ProtocolEnvelope<'env> for UnsignedOriginatorEnvelope {
    type Nested<'a> = PayerEnvelope;
    fn accept<V: super::EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_unsigned_originator(self)?;
        let payer = self.get_nested()?;
        payer.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(PayerEnvelope::decode(self.payer_envelope_bytes.as_slice())?)
    }
}

impl<'env> ProtocolEnvelope<'env> for PayerEnvelope {
    type Nested<'a> = ClientEnvelope;

    fn accept<V: super::EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_payer(self)?;
        let client = self.get_nested()?;
        client.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(ClientEnvelope::decode(
            self.unsigned_client_envelope.as_slice(),
        )?)
    }
}

impl<'env> ProtocolEnvelope<'env> for ClientEnvelope {
    type Nested<'a> = Option<&'a Payload>;

    fn accept<V: super::EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_client(self)?;
        self.get_nested()?.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        // TODO: if Payload being missing needs to be handled, we
        // should return an error here and modify the type of Nested.
        Ok(self.payload.as_ref())
    }
}

impl<'env> ProtocolEnvelope<'env> for Payload {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        match self {
            Payload::GroupMessage(msg) => msg.accept(visitor)?,
            Payload::WelcomeMessage(msg) => msg.accept(visitor)?,
            Payload::UploadKeyPackage(msg) => msg.accept(visitor)?,
            Payload::IdentityUpdate(msg) => msg.accept(visitor)?,
            Payload::PayerReport(_) => return Ok(()),
            Payload::PayerReportAttestation(_) => return Ok(()),
        };
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for GroupMessageInput {
    type Nested<'a> = Option<&'a GroupMessageVersion>;

    fn accept<'a, V: EnvelopeVisitor<'env>>(&'a self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_group_message_input(self)?;
        if let Some(versioned) = self.get_nested()? {
            versioned.accept(visitor)?;
        }
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        // TODO: if GroupMessageVersion being missing needs  to be handled, we
        // should return an error here.
        Ok(self.version.as_ref())
    }
}

// TODO(cvoell): impl<'env> ProtocolEnvelope<'env> for BatchPublishCommitLogRequest {

impl<'env> ProtocolEnvelope<'env> for GroupMessageVersion {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_group_message_version(self)?;
        match self {
            GroupMessageVersion::V1(v1) => visitor.visit_group_message_v1(v1),
        }?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for WelcomeMessageInput {
    type Nested<'a> = Option<&'a WelcomeMessageVersion>;

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_welcome_message_input(self)?;
        self.get_nested()?.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        // TODO: if WelcomeMessageVersion being missing needs  to be handled, we
        // should return an error here and modify the return type of Nested
        Ok(self.version.as_ref())
    }
}

impl<'env> ProtocolEnvelope<'env> for WelcomeMessageVersion {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_welcome_message_version(self)?;
        match self {
            WelcomeMessageVersion::V1(v1) => visitor.visit_welcome_message_v1(v1),
            WelcomeMessageVersion::WelcomePointer(_) => todo!("WelcomePointer not supported"),
        }?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for UploadKeyPackageRequest {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_upload_key_package(self)?;
        self.get_nested()?.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for IdentityUpdate {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_identity_update(self)?;
        self.get_nested()?.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for get_identity_updates_request::Request {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_identity_updates_request(self)?;
        self.get_nested()?.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for get_newest_envelope_response::Response {
    type Nested<'a> = Option<&'a OriginatorEnvelope>;

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_newest_envelope_response(self)?;
        self.get_nested()?.accept(visitor)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(self.originator_envelope.as_ref())
    }
}

impl<'env, T> ProtocolEnvelope<'env> for Option<&T>
where
    T: ProtocolEnvelope<'env>,
{
    type Nested<'a>
        = ()
    where
        Self: 'a;

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        match self {
            Some(o) => o.accept(visitor),
            None => Ok(visitor.visit_none()?),
        }
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for SubscribeGroupMessagesRequest {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        for filter in &self.filters {
            visitor.visit_subscribe_group_messages_request(filter)?;
        }
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for SubscribeWelcomeMessagesRequest {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        for filter in &self.filters {
            visitor.visit_subscribe_welcome_messages_request(filter)?;
        }
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for () {
    type Nested<'a> = ();

    fn accept<V: super::EnvelopeVisitor<'env>>(&self, _: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}
