use chrono::{DateTime, Local};
use xmtp_proto::ConversionError;
use xmtp_proto::types::{Cursor, InstallationId, WelcomeMessage, WelcomeMessageBuilder};

use crate::protocol::traits::EnvelopeVisitor;
use crate::protocol::{ExtractionError, Extractor};
use xmtp_proto::mls_v1::welcome_message_input::V1 as WelcomeMessageV1;
use xmtp_proto::xmtp::xmtpv4::envelopes::UnsignedOriginatorEnvelope;

/// Type to extract a Welcome Message from Originator Envelopes
#[derive(Default)]
pub struct WelcomeMessageExtractor {
    cursor: Cursor,
    created_ns: DateTime<Local>,
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
            Ok(gm.build().unwrap())
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
        self.cursor = Cursor {
            originator_id: envelope.originator_node_id,
            sequence_id: envelope.originator_sequence_id,
        };
        self.created_ns = DateTime::from_timestamp_nanos(envelope.originator_ns).into();
        Ok(())
    }

    fn visit_welcome_message_v1(&mut self, message: &WelcomeMessageV1) -> Result<(), Self::Error> {
        let mut wm = WelcomeMessageBuilder::default();
        wm.installation_key(InstallationId::try_from(message.installation_key.clone())?)
            .data(message.data.clone())
            .hpke_public_key(message.hpke_public_key.clone())
            .wrapper_algorithm(message.wrapper_algorithm)
            .welcome_metadata(message.welcome_metadata.clone());
        self.welcome_message = Some(wm);
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
            .cursor(Cursor {
                originator_id: originator_node_id,
                sequence_id: message.id,
            })
            .created_ns(DateTime::from_timestamp_nanos(message.created_ns as i64))
            .installation_key(InstallationId::try_from(message.installation_key.clone())?)
            .data(message.data.clone())
            .hpke_public_key(message.hpke_public_key.clone())
            .wrapper_algorithm(message.wrapper_algorithm)
            .welcome_metadata(message.welcome_metadata.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
                1,
                vec![1, 2, 3],
            )
            .build();

        let mut extractor = WelcomeMessageExtractor::default();
        envelope.accept(&mut extractor).unwrap();
        let welcome_message = extractor.get();

        let msg = welcome_message.unwrap();
        assert_eq!(
            msg.cursor,
            Cursor {
                sequence_id: 456,
                originator_id: 123
            }
        );
        assert_eq!(msg.created_ns.timestamp_nanos_opt().unwrap(), 789);
        assert_eq!(msg.installation_key, installation_key);
        assert_eq!(msg.data, data);
        assert_eq!(msg.hpke_public_key, hpke_public_key);
        assert_eq!(msg.wrapper_algorithm, 1);
        assert_eq!(msg.welcome_metadata, vec![1, 2, 3]);
    }
}
