use std::sync::Arc;
use xmtp_content_types::{ContentCodec, encoded_content_to_bytes, text::TextCodec};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::groups::{MlsGroup, send_message_opts::SendMessageOpts};

use crate::{
    ConversationID, InboxID, Message, MessageID, MessageReader, XmtpError, client::CoreClient,
};

#[derive(uniffi::Object)]
pub struct Conversations {
    pub(crate) client: Arc<CoreClient>,
    pub(crate) client_key: u64,
}

#[xmtp_macro::sdk_export]
impl Conversations {
    pub async fn create_group(&self, members: Vec<InboxID>) -> Result<Arc<Group>, XmtpError> {
        if self.client.context.is_closed() {
            return Err(XmtpError::closed());
        }
        let members: Vec<String> = members.into_iter().map(|member| member.0).collect();
        let group = self
            .client
            .create_group_with_members(&members, None, None)
            .await
            .map_err(XmtpError::unknown)?;
        Ok(Arc::new(Group {
            inner: group,
            client_key: self.client_key,
        }))
    }
}

#[derive(uniffi::Object)]
pub struct Group {
    pub(crate) inner: MlsGroup<xmtp_mls::MlsContext>,
    pub(crate) client_key: u64,
}

#[xmtp_macro::sdk_export]
impl Group {
    pub fn id(&self) -> ConversationID {
        self.inner.group_id.into()
    }

    pub async fn send_text(&self, text: String) -> Result<MessageID, XmtpError> {
        let content = TextCodec::encode(text).map_err(XmtpError::unknown)?;
        let bytes = encoded_content_to_bytes(content);
        let id = self
            .inner
            .send_message(
                &bytes,
                SendMessageOpts {
                    should_push: true,
                    idempotency_key: None,
                },
            )
            .await
            .map_err(XmtpError::unknown)?;
        MessageID::from_bytes(&id)
    }

    pub async fn messages(&self) -> Result<Vec<Message>, XmtpError> {
        self.inner
            .find_messages(&MsgQueryArgs::default())
            .map_err(XmtpError::unknown)?
            .into_iter()
            .map(|message| Message::from_stored(message, self.client_key))
            .collect()
    }

    pub async fn message_reader(&self) -> Result<Arc<MessageReader>, XmtpError> {
        MessageReader::open(
            self.inner.context.clone(),
            self.inner.group_id,
            self.client_key,
        )
    }
}
