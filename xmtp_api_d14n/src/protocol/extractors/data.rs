//! Extractor for an MLS Data field
use xmtp_proto::ConversionError;
use xmtp_proto::mls_v1::group_message_input::V1 as GroupMessageV1;
use xmtp_proto::mls_v1::welcome_message_input::V1 as WelcomeMessageV1;
use xmtp_proto::xmtp::mls::api::v1::{
    group_message::V1 as V3GroupMessage, welcome_message::V1 as V3WelcomeMessage,
};

use crate::protocol::{EnvelopeVisitor, Extractor};

/// Extract Mls Data from Envelopes
#[derive(Default, Clone, Debug)]
pub struct MlsDataExtractor {
    data: Option<Vec<u8>>,
}

impl MlsDataExtractor {
    pub fn new() -> Self {
        Default::default()
    }
}
impl Extractor for MlsDataExtractor {
    type Output = Result<Vec<u8>, ConversionError>;

    fn get(self) -> Self::Output {
        self.data.ok_or(ConversionError::Missing {
            item: "data",
            r#type: std::any::type_name::<Vec<u8>>(),
        })
    }
}

impl EnvelopeVisitor<'_> for MlsDataExtractor {
    type Error = ConversionError;

    fn visit_welcome_message_v1(&mut self, message: &WelcomeMessageV1) -> Result<(), Self::Error> {
        self.data = Some(message.data.clone());
        Ok(())
    }

    fn visit_group_message_v1(&mut self, message: &GroupMessageV1) -> Result<(), Self::Error> {
        self.data = Some(message.data.clone());
        Ok(())
    }

    fn visit_v3_group_message(&mut self, message: &V3GroupMessage) -> Result<(), Self::Error> {
        self.data = Some(message.data.clone());
        Ok(())
    }

    fn visit_v3_welcome_message(&mut self, message: &V3WelcomeMessage) -> Result<(), Self::Error> {
        self.data = Some(message.data.clone());
        Ok(())
    }
}
