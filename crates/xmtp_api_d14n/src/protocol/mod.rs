//! Items still used by the bidi binding during the backend transition.
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_proto::{ConversionError, backend_v1, mls_v1, types::Topic};

pub trait CursorStore: MaybeSend + MaybeSync {}
#[derive(Clone, Copy, Default, Debug)]
pub struct NoCursorStore;
impl CursorStore for NoCursorStore {}

pub trait Envelope {
    fn topic(&self) -> Result<Topic, ConversionError>;
    fn cursor(&self) -> Result<backend_v1::Cursor, ConversionError>;
}

impl Envelope for mls_v1::GroupMessage {
    fn topic(&self) -> Result<Topic, ConversionError> {
        let Some(mls_v1::group_message::Version::V1(message)) = &self.version else {
            return Err(ConversionError::Unspecified("group message version"));
        };
        Ok(Topic::new_group_message(&message.group_id))
    }
    fn cursor(&self) -> Result<backend_v1::Cursor, ConversionError> {
        let Some(mls_v1::group_message::Version::V1(message)) = &self.version else {
            return Err(ConversionError::Unspecified("group message version"));
        };
        Ok(backend_v1::Cursor {
            sequence_id: message.id,
        })
    }
}

impl Envelope for mls_v1::WelcomeMessage {
    fn topic(&self) -> Result<Topic, ConversionError> {
        let message = self
            .version
            .as_ref()
            .ok_or(ConversionError::Unspecified("welcome version"))?;
        Ok(xmtp_proto::types::TopicKind::WelcomeMessagesV1.create(message.installation_key()))
    }
    fn cursor(&self) -> Result<backend_v1::Cursor, ConversionError> {
        let message = self
            .version
            .as_ref()
            .ok_or(ConversionError::Unspecified("welcome version"))?;
        Ok(backend_v1::Cursor {
            sequence_id: message.id(),
        })
    }
}
