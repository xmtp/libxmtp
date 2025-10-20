//! Implementions of traits
use crate::protocol::EnvelopeCollection;

use super::traits::{EnvelopeError, EnvelopeVisitor, ProtocolEnvelope};
use prost::Message;
use xmtp_proto::identity_v1::get_identity_updates_response::IdentityUpdateLog;
use xmtp_proto::mls_v1::fetch_key_packages_response::KeyPackage;
use xmtp_proto::mls_v1::subscribe_group_messages_request::Filter as SubscribeGroupMessagesFilter;
use xmtp_proto::mls_v1::subscribe_welcome_messages_request::Filter as SubscribeWelcomeMessagesFilter;
use xmtp_proto::mls_v1::{
    SubscribeGroupMessagesRequest, SubscribeWelcomeMessagesRequest, welcome_message,
};
use xmtp_proto::types::Topic;
use xmtp_proto::xmtp::xmtpv4::message_api::{
    SubscribeEnvelopesResponse, get_newest_envelope_response,
};
use xmtp_proto::{
    ConversionError,
    xmtp::identity::{api::v1::get_identity_updates_request, associations::IdentityUpdate},
    xmtp::mls::api::v1::UploadKeyPackageRequest,
    xmtp::mls::api::v1::{
        GroupMessage as V3ProtoGroupMessage, GroupMessageInput,
        WelcomeMessage as V3ProtoWelcomeMessage, WelcomeMessageInput, group_message,
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
            Payload::PayerReport(_) => {
                tracing::warn!("Payload::PayerReport type not handled in client");
                return Ok(());
            }
            Payload::PayerReportAttestation(_) => {
                tracing::warn!("Payload::PayerReportAttestation type not handled in client");
                return Ok(());
            }
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

impl<'env> ProtocolEnvelope<'env> for KeyPackage {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_key_package(self)?;
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

impl<'env> ProtocolEnvelope<'env> for SubscribeGroupMessagesFilter {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_subscribe_group_messages_request(self)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for SubscribeWelcomeMessagesFilter {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_subscribe_welcome_messages_request(self)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for V3ProtoGroupMessage {
    type Nested<'a> = Option<&'a group_message::Version>;

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        self.get_nested()?.accept(visitor)
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(self.version.as_ref())
    }
}

impl<'env> ProtocolEnvelope<'env> for group_message::Version {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        match self {
            group_message::Version::V1(v1) => visitor.visit_v3_group_message(v1)?,
        }
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for V3ProtoWelcomeMessage {
    type Nested<'a> = Option<&'a welcome_message::Version>;

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        self.get_nested()?.accept(visitor)
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(self.version.as_ref())
    }
}

impl<'env> ProtocolEnvelope<'env> for welcome_message::Version {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        match self {
            welcome_message::Version::V1(v1) => visitor.visit_v3_welcome_message(v1)?,
        }
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(())
    }
}

impl<'env> ProtocolEnvelope<'env> for IdentityUpdateLog {
    type Nested<'a> = Option<&'a IdentityUpdate>;

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.visit_identity_update_log(self)?;
        self.get_nested()?.accept(visitor)
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, ConversionError> {
        Ok(self.update.as_ref())
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

impl EnvelopeCollection<'_> for SubscribeEnvelopesResponse {
    fn topics(&self) -> Result<Vec<Topic>, EnvelopeError> {
        self.envelopes.topics()
    }

    fn payloads(&self) -> Result<Vec<Payload>, EnvelopeError> {
        self.envelopes.payloads()
    }

    fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError> {
        self.envelopes.client_envelopes()
    }

    fn len(&self) -> usize {
        self.envelopes.len()
    }

    fn is_empty(&self) -> bool {
        self.envelopes.is_empty()
    }

    fn consume<E>(self) -> Result<Vec<<E as super::Extractor>::Output>, EnvelopeError>
    where
        for<'a> E: Default + super::Extractor + EnvelopeVisitor<'a>,
        Self: Clone,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
        Self: Sized,
    {
        self.envelopes.consume::<E>()
    }

    fn group_messages(
        &self,
    ) -> Result<Vec<Option<xmtp_proto::types::GroupMessage>>, EnvelopeError> {
        self.envelopes.group_messages()
    }

    fn welcome_messages(
        &self,
    ) -> Result<Vec<Option<xmtp_proto::types::WelcomeMessage>>, EnvelopeError> {
        self.envelopes.welcome_messages()
    }
}

impl EnvelopeCollection<'_> for SubscribeGroupMessagesRequest {
    fn topics(&self) -> Result<Vec<Topic>, EnvelopeError> {
        self.filters.topics()
    }

    fn payloads(&self) -> Result<Vec<Payload>, EnvelopeError> {
        self.filters.payloads()
    }

    fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError> {
        self.filters.client_envelopes()
    }

    fn len(&self) -> usize {
        self.filters.len()
    }

    fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    fn consume<E>(self) -> Result<Vec<<E as super::Extractor>::Output>, EnvelopeError>
    where
        for<'a> E: Default + super::Extractor + EnvelopeVisitor<'a>,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
        Self: Sized,
    {
        self.filters.consume()
    }

    fn group_messages(
        &self,
    ) -> Result<Vec<Option<xmtp_proto::types::GroupMessage>>, EnvelopeError> {
        self.filters.group_messages()
    }

    fn welcome_messages(
        &self,
    ) -> Result<Vec<Option<xmtp_proto::types::WelcomeMessage>>, EnvelopeError> {
        self.filters.welcome_messages()
    }
}

impl EnvelopeCollection<'_> for SubscribeWelcomeMessagesRequest {
    fn topics(&self) -> Result<Vec<Topic>, EnvelopeError> {
        self.filters.topics()
    }

    fn payloads(&self) -> Result<Vec<Payload>, EnvelopeError> {
        self.filters.payloads()
    }

    fn client_envelopes(&self) -> Result<Vec<ClientEnvelope>, EnvelopeError> {
        self.filters.client_envelopes()
    }

    fn len(&self) -> usize {
        self.filters.len()
    }

    fn is_empty(&self) -> bool {
        self.filters.is_empty()
    }

    fn consume<E>(self) -> Result<Vec<<E as super::Extractor>::Output>, EnvelopeError>
    where
        for<'a> E: Default + super::Extractor + EnvelopeVisitor<'a>,
        for<'a> EnvelopeError: From<<E as EnvelopeVisitor<'a>>::Error>,
        Self: Sized,
    {
        self.filters.consume()
    }

    fn group_messages(
        &self,
    ) -> Result<Vec<Option<xmtp_proto::types::GroupMessage>>, EnvelopeError> {
        self.filters.group_messages()
    }

    fn welcome_messages(
        &self,
    ) -> Result<Vec<Option<xmtp_proto::types::WelcomeMessage>>, EnvelopeError> {
        self.filters.welcome_messages()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl<'env> ProtocolEnvelope<'env> for u32 {
    type Nested<'a> = ();

    fn accept<V: EnvelopeVisitor<'env>>(&self, visitor: &mut V) -> Result<(), EnvelopeError>
    where
        EnvelopeError: From<<V as EnvelopeVisitor<'env>>::Error>,
    {
        visitor.test_visit_u32(self)?;
        Ok(())
    }

    fn get_nested(&self) -> Result<Self::Nested<'_>, xmtp_proto::ConversionError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Envelope;
    use crate::protocol::extractors::test_utils::*;
    use rstest::rstest;
    use xmtp_common::Generate;
    use xmtp_cryptography::XmtpInstallationCredential;
    use xmtp_proto::types::TopicKind;
    use xmtp_proto::xmtp::mls::api::v1::{
        GroupMessage as V3ProtoGroupMessage, WelcomeMessage as V3ProtoWelcomeMessage,
        group_message, group_message_input::V1 as GroupMessageV1, welcome_message,
        welcome_message_input::V1 as WelcomeMessageV1,
    };
    use xmtp_proto::xmtp::xmtpv4::envelopes::AuthenticatedData;

    /// Minimal test visitor that tracks which methods were called
    #[derive(Default, Debug)]
    struct TestVisitor {
        visited_originator: bool,
        visited_client: bool,
        visited_group_message_v1: bool,
        visited_welcome_message_v1: bool,
        visited_upload_key_package: bool,
        visited_identity_update: bool,
        visited_none: bool,
        visited_v3_group_message: bool,
        visited_v3_welcome_message: bool,
    }

    impl<'env> EnvelopeVisitor<'env> for TestVisitor {
        type Error = super::EnvelopeError;

        fn visit_originator(&mut self, _e: &OriginatorEnvelope) -> Result<(), Self::Error> {
            self.visited_originator = true;
            Ok(())
        }

        fn visit_client(&mut self, _e: &ClientEnvelope) -> Result<(), Self::Error> {
            self.visited_client = true;
            Ok(())
        }

        fn visit_group_message_v1(&mut self, _m: &GroupMessageV1) -> Result<(), Self::Error> {
            self.visited_group_message_v1 = true;
            Ok(())
        }

        fn visit_welcome_message_v1(&mut self, _m: &WelcomeMessageV1) -> Result<(), Self::Error> {
            self.visited_welcome_message_v1 = true;
            Ok(())
        }

        fn visit_upload_key_package(
            &mut self,
            _p: &UploadKeyPackageRequest,
        ) -> Result<(), Self::Error> {
            self.visited_upload_key_package = true;
            Ok(())
        }

        fn visit_identity_update(&mut self, _u: &IdentityUpdate) -> Result<(), Self::Error> {
            self.visited_identity_update = true;
            Ok(())
        }

        fn visit_none(&mut self) -> Result<(), Self::Error> {
            self.visited_none = true;
            Ok(())
        }

        fn visit_v3_group_message(&mut self, _m: &group_message::V1) -> Result<(), Self::Error> {
            self.visited_v3_group_message = true;
            Ok(())
        }

        fn visit_v3_welcome_message(
            &mut self,
            _m: &welcome_message::V1,
        ) -> Result<(), Self::Error> {
            self.visited_v3_welcome_message = true;
            Ok(())
        }
    }

    #[rstest]
    #[case::group_message(
        |builder: TestEnvelopeBuilder| builder.with_application_message(vec![1,2,3]),
        |visitor: &TestVisitor| visitor.visited_group_message_v1,
    )]
    #[case::welcome_message(
        |builder: TestEnvelopeBuilder| builder.with_welcome_message(vec![1,2,3,4]),
        |visitor: &TestVisitor| visitor.visited_welcome_message_v1,
    )]
    #[case::key_package(
        |builder: TestEnvelopeBuilder| builder.with_key_package("test".to_string(), XmtpInstallationCredential::default()),
        |visitor: &TestVisitor| visitor.visited_upload_key_package,
    )]
    #[case::identity_update(
        |builder: TestEnvelopeBuilder| builder.with_identity_update(),
        |visitor: &TestVisitor| visitor.visited_identity_update,
    )]
    #[xmtp_common::test]
    async fn envelope_visitor_flows<F, P>(#[case] envelope_builder: F, #[case] payload_check: P)
    where
        F: Fn(TestEnvelopeBuilder) -> TestEnvelopeBuilder,
        P: Fn(&TestVisitor) -> bool,
    {
        let envelope = envelope_builder(TestEnvelopeBuilder::new()).build();
        let mut visitor = TestVisitor::default();
        envelope.accept(&mut visitor).unwrap();

        // All envelopes should visit these core types
        assert!(visitor.visited_originator, "originator should be visited");
        assert!(visitor.visited_client, "client should be visited");

        // Payload-specific assertion
        assert!(
            payload_check(&visitor),
            "payload-specific visitor should be called"
        );

        // Extraction should work
        assert!(envelope.topic().is_ok(), "topic extraction should work");
        assert!(envelope.payload().is_ok(), "payload extraction should work");
        assert!(
            envelope.client_envelope().is_ok(),
            "client envelope extraction should work"
        );
    }

    #[xmtp_common::test]
    fn envelope_collections() {
        let envelopes = vec![
            TestEnvelopeBuilder::new()
                .with_application_message(vec![1, 2, 3])
                .build(),
            TestEnvelopeBuilder::new()
                .with_welcome_message(vec![4, 5, 6, 7])
                .build(),
            TestEnvelopeBuilder::new()
                .with_key_package("test".to_string(), XmtpInstallationCredential::default())
                .build(),
            TestEnvelopeBuilder::new().with_identity_update().build(),
        ];
        // Test EnvelopeCollection implementations
        let response = SubscribeEnvelopesResponse {
            envelopes: envelopes.clone(),
        };
        assert_eq!(response.len(), 4);
        assert_eq!(response.payloads().unwrap().len(), 4);
        assert_eq!(response.client_envelopes().unwrap().len(), 4);
        assert!(!response.is_empty());

        let empty_response = SubscribeEnvelopesResponse { envelopes: vec![] };
        assert_eq!(empty_response.len(), 0);
        assert!(empty_response.is_empty());
    }

    #[xmtp_common::test]
    fn envelope_error_handling() {
        // Test corrupted originator envelope
        let mut envelope = TestEnvelopeBuilder::new()
            .with_application_message(vec![1, 2, 3])
            .build();
        envelope.unsigned_originator_envelope = vec![0xFF];
        assert!(envelope.get_nested().is_err());

        // Test corrupted payer envelope
        let unsigned = UnsignedOriginatorEnvelope {
            originator_node_id: 1,
            originator_sequence_id: 1,
            originator_ns: 1000,
            payer_envelope_bytes: vec![0xFF, 0xFF, 0xFF],
            base_fee_picodollars: 0,
            congestion_fee_picodollars: 0,
            expiry_unixtime: 0,
        };
        assert!(unsigned.get_nested().is_err());

        // Test corrupted client envelope
        let payer = PayerEnvelope {
            unsigned_client_envelope: vec![0xFF, 0xFF, 0xFF],
            payer_signature: None,
            target_originator: 0,
            message_retention_days: 30,
        };
        assert!(payer.get_nested().is_err());
    }

    #[xmtp_common::test]
    fn envelope_edge_cases() {
        // Test empty payload handling
        let client = ClientEnvelope {
            aad: Some(AuthenticatedData::with_topic(
                TopicKind::IdentityUpdatesV1.create([0, 1, 2]),
            )),
            payload: None,
        };
        let mut visitor = TestVisitor::default();
        client.accept(&mut visitor).unwrap();
        assert!(visitor.visited_client && visitor.visited_none);

        // Test Option<T> ProtocolEnvelope implementation
        let envelope = TestEnvelopeBuilder::new()
            .with_application_message(vec![1, 2, 3])
            .build();
        let some_envelope = Some(&envelope);
        let none_envelope: Option<&OriginatorEnvelope> = None;

        let mut visitor = TestVisitor::default();
        some_envelope.accept(&mut visitor).unwrap();
        assert!(visitor.visited_originator && !visitor.visited_none);

        let mut visitor = TestVisitor::default();
        none_envelope.accept(&mut visitor).unwrap();
        assert!(visitor.visited_none && !visitor.visited_originator);
    }

    #[xmtp_common::test]
    fn test_v3_message_visitors() {
        macro_rules! test_case {
            ($msg:expr, $expected:expr, $field:ident) => {{
                let mut visitor = TestVisitor::default();
                $msg.accept(&mut visitor).unwrap();
                assert_eq!(
                    visitor.$field,
                    $expected,
                    "V3 {} should be {}",
                    stringify!($field),
                    $expected
                );
            }};
        }

        // Test all V3 message visitor combinations
        test_case!(
            V3ProtoGroupMessage {
                version: Some(group_message::Version::V1(group_message::V1::generate()))
            },
            true,
            visited_v3_group_message
        );
        test_case!(
            V3ProtoGroupMessage { version: None },
            false,
            visited_v3_group_message
        );
        test_case!(
            V3ProtoWelcomeMessage {
                version: Some(welcome_message::Version::V1(welcome_message::V1::generate()))
            },
            true,
            visited_v3_welcome_message
        );
        test_case!(
            V3ProtoWelcomeMessage { version: None },
            false,
            visited_v3_welcome_message
        );
        test_case!(
            group_message::Version::V1(group_message::V1::generate()),
            true,
            visited_v3_group_message
        );
        test_case!(
            welcome_message::Version::V1(welcome_message::V1::generate()),
            true,
            visited_v3_welcome_message
        );
    }
}
