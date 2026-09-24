use std::future::Future;
use std::sync::Arc;
use xmtp_content_types::{ContentCodec, encoded_content_to_bytes, text::TextCodec};
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_mls::MlsContext;
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::groups::{MlsGroup, send_message_opts::SendMessageOpts};

use crate::{
    ConversationID, InboxID, Message, MessageID, MessageReader, XmtpError, client::CoreClient,
};

// Native calls run on an owned task in every profile. This gives nested MLS
// work a fresh executor stack and lets it finish if the FFI call is cancelled.
#[cfg(not(target_arch = "wasm32"))]
async fn on_sdk_worker<T, F>(context: MlsContext, work: F) -> Result<T, XmtpError>
where
    T: Send + 'static,
    F: Future<Output = Result<T, XmtpError>> + Send + 'static,
{
    tokio::spawn(while_open(context, work))
        .await
        .map_err(XmtpError::unknown)?
}

#[cfg(target_arch = "wasm32")]
async fn on_sdk_worker<T, F>(context: MlsContext, work: F) -> Result<T, XmtpError>
where
    F: Future<Output = Result<T, XmtpError>>,
{
    while_open(context, work).await
}

// `end()` can start while the task waits to run, so check the closed state in
// the task. `end()` only cancels a token: work that passes this check can still
// finish after `end()` starts. A failure after that point is `ClientClosed`.
async fn while_open<T, F>(context: MlsContext, work: F) -> Result<T, XmtpError>
where
    F: Future<Output = Result<T, XmtpError>>,
{
    if context.is_closed() {
        return Err(XmtpError::closed());
    }
    work.await.map_err(|error| {
        if context.is_closed() {
            XmtpError::closed()
        } else {
            error
        }
    })
}

#[derive(uniffi::Object)]
pub struct Conversations {
    pub(crate) client: Arc<CoreClient>,
    pub(crate) client_key: u64,
}

#[xmtp_macro::sdk_export]
impl Conversations {
    pub async fn create_group(&self, members: Vec<InboxID>) -> Result<Arc<Group>, XmtpError> {
        let members: Vec<String> = members.into_iter().map(|member| member.0).collect();
        let client = self.client.clone();
        let group = on_sdk_worker(self.client.context.clone(), async move {
            client
                .create_group_with_members(&members, None, None)
                .await
                .map_err(XmtpError::unknown)
        })
        .await?;
        Ok(Arc::new(Group {
            inner: group,
            client_key: self.client_key,
        }))
    }
}

#[cfg(feature = "bench")]
#[xmtp_macro::sdk_export]
impl Conversations {
    /// Open a group already stored in this client's database for the benchmark.
    pub fn get_group(&self, id: ConversationID) -> Result<Arc<Group>, XmtpError> {
        if self.client.context.is_closed() {
            return Err(XmtpError::closed());
        }
        let bytes = hex::decode(id.0).map_err(XmtpError::unknown)?;
        let group_id = xmtp_proto::types::GroupId::try_from(bytes).map_err(XmtpError::unknown)?;
        let group = self.client.group(&group_id).map_err(XmtpError::unknown)?;
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
        let group = self.inner.clone();
        on_sdk_worker(group.context.clone(), async move {
            let id = group
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
        })
        .await
    }

    pub async fn messages(&self) -> Result<Vec<Message>, XmtpError> {
        let group = self.inner.clone();
        let client_key = self.client_key;
        on_sdk_worker(group.context.clone(), async move {
            group
                .find_messages(&MsgQueryArgs::default())
                .map_err(XmtpError::unknown)?
                .into_iter()
                .map(|message| Message::from_stored(message, client_key))
                .collect()
        })
        .await
    }

    pub async fn message_reader(&self) -> Result<Arc<MessageReader>, XmtpError> {
        let context = self.inner.context.clone();
        let group_id = self.inner.group_id;
        let client_key = self.client_key;
        on_sdk_worker(context.clone(), async move {
            MessageReader::open(context, group_id, client_key)
        })
        .await
    }
}
