use chrono::{DateTime, Utc};
use xmtp_proto::ConversionError;
use xmtp_proto::types::{
    Cursor, WelcomeMessage, WelcomeMessageBuilder, WelcomeMessageV1, WelcomePointer,
};

use crate::protocol::traits::EnvelopeVisitor;
use crate::protocol::{ExtractionError, Extractor};
use xmtp_proto::mls_v1::welcome_message::WelcomePointer as V3ProtoWelcomePointer;
use xmtp_proto::mls_v1::welcome_message_input::{
    V1 as ProtoWelcomeMessageV1, WelcomePointer as WelcomeMessageWelcomePointer,
};
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

/// Type to extract a Welcome Message from Originator Envelopes
#[derive(Default)]
pub struct WelcomeMessageExtractor {
    cursor: Cursor,
    created_ns: DateTime<Utc>,
    welcome_message: Option<WelcomeMessageBuilder>,
}

impl Extractor for WelcomeMessageExtractor {
    type Output = Result<WelcomeMessage, ExtractionError>;

    fn get(self) -> Self::Output {
        let Self {
            cursor,
            created_ns,
            welcome_message,
        } = self;
        if let Some(mut gm) = welcome_message {
            gm.cursor(cursor);
            gm.created_ns(created_ns);
            Ok(gm.build()?)
        } else {
            Err(ExtractionError::Conversion(ConversionError::Missing {
                item: "welcome_message",
                r#type: std::any::type_name::<WelcomeMessage>(),
            }))
        }
    }
}

impl EnvelopeVisitor<'_> for WelcomeMessageExtractor {
    type Error = ConversionError;

    fn visit_unsigned_originator(
        &mut self,
        envelope: &UnsignedOriginatorEnvelope,
    ) -> Result<(), Self::Error> {
        self.cursor = Cursor::new(envelope.originator_sequence_id, envelope.originator_node_id);
        self.created_ns = DateTime::from_timestamp_nanos(envelope.originator_ns);
        Ok(())
    }

    fn visit_welcome_message_v1(
        &mut self,
        message: &ProtoWelcomeMessageV1,
    ) -> Result<(), Self::Error> {
        let mut builder = WelcomeMessage::builder();
        builder.variant(WelcomeMessageV1 {
            installation_key: message.installation_key.as_slice().try_into()?,
            data: message.data.clone(),
            hpke_public_key: message.hpke_public_key.clone(),
            wrapper_algorithm: message.wrapper_algorithm.try_into()?,
            welcome_metadata: message.welcome_metadata.clone(),
        });
        self.welcome_message = Some(builder);
        Ok(())
    }

    fn visit_welcome_pointer(
        &mut self,
        message: &WelcomeMessageWelcomePointer,
    ) -> Result<(), Self::Error> {
        let mut builder = WelcomeMessage::builder();
        builder.variant(WelcomePointer {
            installation_key: message.installation_key.as_slice().try_into()?,
            welcome_pointer: message.welcome_pointer.clone(),
            hpke_public_key: message.hpke_public_key.clone(),
            wrapper_algorithm: message.wrapper_algorithm.try_into()?,
        });
        self.welcome_message = Some(builder);
        Ok(())
    }
}

#[derive(Default)]
pub struct V3WelcomeMessageExtractor {
    welcome_message: WelcomeMessageBuilder,
}

impl Extractor for V3WelcomeMessageExtractor {
    type Output = Result<WelcomeMessage, ConversionError>;

    fn get(self) -> Self::Output {
        self.welcome_message.build()
    }
}

impl EnvelopeVisitor<'_> for V3WelcomeMessageExtractor {
    type Error = ConversionError;

    fn visit_v3_welcome_message(
        &mut self,
        message: &xmtp_proto::mls_v1::welcome_message::V1,
    ) -> Result<(), Self::Error> {
        let originator_node_id = xmtp_configuration::Originators::WELCOME_MESSAGES;

        self.welcome_message
            .cursor(Cursor::new(message.id, originator_node_id))
            .created_ns(DateTime::from_timestamp_nanos(message.created_ns as i64))
            .variant(
                WelcomeMessageV1::builder()
                    .installation_key(message.installation_key.as_slice().try_into()?)
                    .data(message.data.clone())
                    .hpke_public_key(message.hpke_public_key.clone())
                    .wrapper_algorithm(message.wrapper_algorithm.try_into()?)
                    .welcome_metadata(message.welcome_metadata.clone())
                    .build()?,
            );
        Ok(())
    }

    fn visit_v3_welcome_pointer(
        &mut self,
        message: &V3ProtoWelcomePointer,
    ) -> Result<(), Self::Error> {
        let originator_node_id = xmtp_configuration::Originators::WELCOME_MESSAGES;
        self.welcome_message
            .cursor(Cursor::new(message.id, originator_node_id))
            .created_ns(DateTime::from_timestamp_nanos(message.created_ns as i64))
            .variant(
                WelcomePointer::builder()
                    .installation_key(message.installation_key.as_slice().try_into()?)
                    .welcome_pointer(message.welcome_pointer.clone())
                    .hpke_public_key(message.hpke_public_key.clone())
                    .wrapper_algorithm(message.wrapper_algorithm.try_into()?)
                    .build()?,
            );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperAlgorithm;

    use super::*;
    use crate::protocol::ProtocolEnvelope;
    use crate::protocol::extractors::test_utils::*;

    #[xmtp_common::test]
    fn test_extract_welcome_message() {
        let installation_key = xmtp_common::rand_vec::<32>();
        let data = xmtp_common::rand_vec::<64>();
        let hpke_public_key = xmtp_common::rand_vec::<32>();

        let envelope = TestEnvelopeBuilder::new()
            .with_originator_node_id(123)
            .with_originator_sequence_id(456)
            .with_originator_ns(789)
            .with_welcome_message_full(
                installation_key.clone(),
                data.clone(),
                hpke_public_key.clone(),
                WelcomeWrapperAlgorithm::XwingMlkem768Draft6.into(),
                vec![1, 2, 3],
            )
            .build();

        let mut extractor = WelcomeMessageExtractor::default();
        envelope.accept(&mut extractor).unwrap();
        let welcome_message = extractor.get();

        let msg = welcome_message.unwrap();
        assert_eq!(msg.cursor, Cursor::new(456u64, 123u32));
        assert_eq!(msg.created_ns.timestamp_nanos_opt().unwrap(), 789);
        let v1 = msg.as_v1().unwrap();
        assert_eq!(v1.installation_key, installation_key);
        assert_eq!(v1.data, data);
        assert_eq!(v1.hpke_public_key, hpke_public_key);
        assert_eq!(
            v1.wrapper_algorithm,
            WelcomeWrapperAlgorithm::XwingMlkem768Draft6
        );
        assert_eq!(v1.welcome_metadata, vec![1, 2, 3]);
    }
}
