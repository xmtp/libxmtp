use std::future::Future;
use std::sync::Arc;
use xmtp_content_types::{
    ContentCodec,
    compression::compress_if_requested,
    encoded_content_to_bytes,
    reaction::ReactionCodec,
    reply::{Reply, ReplyCodec},
};
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::group_message::MsgQueryArgs;
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::prelude::QueryDms;
use xmtp_db::prelude::QueryGroup;
use xmtp_db::prelude::QueryGroupMessage;
use xmtp_mls::MlsContext;
use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::groups::{MlsGroup, send_message_opts::SendMessageOpts};
use xmtp_mls::messages::decoded_message::{DecodedMessage, MessageBody as CoreMessageBody};
use xmtp_mls::mls_store::MlsStore;
use xmtp_proto::types::{ConversationType, GroupId};

use crate::{
    ConsentState, ContentTypeId, ConversationHmacKeys, ConversationID, ConversationState,
    CreateDmOptions, CreateGroupOptions, DisappearingSettings, EncodedContent, GroupState,
    GroupSyncSummary, HmacKey, InboxID, LastReadTimeEntry, ListConversationsOptions,
    ListMessagesOptions, Member, Message, MessageID, MessageReader, NotificationOverride,
    PublicIdentity, Reaction, SendOptions, StandardContent, Timestamp, XmtpError,
    client::CoreClient,
};

// Native calls run on an owned task in every profile. This keeps SQLite work
// off the JavaScript thread, gives nested MLS work a fresh executor stack, and
// lets work finish if the FFI call is cancelled. On wasm32, cancellation drops
// the work because the target has no blocking thread pool.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn on_sdk_worker<T, F>(context: MlsContext, work: F) -> Result<T, XmtpError>
where
    T: Send + 'static,
    F: Future<Output = Result<T, XmtpError>> + Send + 'static,
{
    tokio::spawn(while_open(context, work))
        .await
        .map_err(XmtpError::unknown)?
}

#[cfg(target_arch = "wasm32")]
pub(crate) async fn on_sdk_worker<T, F>(context: MlsContext, work: F) -> Result<T, XmtpError>
where
    F: Future<Output = Result<T, XmtpError>>,
{
    while_open(context, work).await
}

// Check the closed state in the task because end() can run before it starts.
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

fn deletion_group(
    group: MlsGroup<MlsContext>,
    stored: &StoredGroupMessage,
) -> Result<MlsGroup<MlsContext>, XmtpError> {
    if group.conversation_type == ConversationType::Dm
        && stored.sender_inbox_id != group.context.inbox_id()
    {
        return Err(XmtpError::conversation_permission_denied(
            "not your message",
        ));
    }
    if stored.group_id == group.group_id {
        return Ok(group);
    }
    let stitched = group
        .context
        .db()
        .fetch_stitched(&stored.group_id)
        .map_err(XmtpError::unknown)?;
    if stitched.is_none_or(|winner| winner.id != group.group_id) {
        return Err(XmtpError::conversation_permission_denied(
            "message belongs to another conversation",
        ));
    }
    MlsStore::new(group.context.clone())
        .group(&stored.group_id)
        .map_err(XmtpError::unknown)
}

#[derive(uniffi::Object)]
pub struct Conversations {
    pub(crate) client: Arc<CoreClient>,
    pub(crate) client_key: u64,
}

#[derive(Clone, uniffi::Enum)]
pub enum Conversation {
    Group { group: Arc<Group> },
    Dm { dm: Arc<Dm> },
}

impl Conversation {
    async fn from_core(
        group: MlsGroup<xmtp_mls::MlsContext>,
        client_key: u64,
    ) -> Result<Option<Self>, XmtpError> {
        Ok(match group.conversation_type {
            ConversationType::Group => Some(Self::Group {
                group: Arc::new(Group::from_core(group, client_key).await?),
            }),
            ConversationType::Dm => Some(Self::Dm {
                dm: Arc::new(Dm::from_core(group, client_key).await?),
            }),
            ConversationType::Sync | ConversationType::Oneshot => None,
        })
    }
}

#[xmtp_macro::sdk_export]
impl Conversations {
    pub async fn create_group_optimistic(
        &self,
        options: Option<CreateGroupOptions>,
    ) -> Result<Arc<Group>, XmtpError> {
        let (permissions, metadata) = options.unwrap_or_default().into_core()?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let group = client
                .create_group(permissions, Some(metadata))
                .map_err(XmtpError::unknown)?;
            Ok(Arc::new(Group::from_core(group, client_key).await?))
        })
        .await
    }

    pub async fn get_dm_by_inbox_id(&self, peer: InboxID) -> Result<Option<Arc<Dm>>, XmtpError> {
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let members = xmtp_mls::mls_common::group_metadata::DmMembers {
                member_one_inbox_id: client.inbox_id(),
                member_two_inbox_id: peer.0.as_str(),
            };
            let Some(stored) = client
                .context
                .db()
                .find_active_dm_group(&members)
                .map_err(XmtpError::unknown)?
            else {
                return Ok(None);
            };
            let group = client.group(&stored.id).map_err(XmtpError::unknown)?;
            Ok(Some(Arc::new(Dm::from_core(group, client_key).await?)))
        })
        .await
    }

    pub async fn get_dm_by_identity(
        &self,
        identity: PublicIdentity,
    ) -> Result<Option<Arc<Dm>>, XmtpError> {
        let client = self.client.clone();
        let identifier = identity.to_core()?;
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let Some(peer) = client
                .find_inbox_id_from_identifier(&client.context.db(), identifier)
                .await
                .map_err(XmtpError::from_client)?
            else {
                return Ok(None);
            };
            let members = xmtp_mls::mls_common::group_metadata::DmMembers {
                member_one_inbox_id: client.inbox_id(),
                member_two_inbox_id: peer.as_str(),
            };
            let Some(stored) = client
                .context
                .db()
                .find_active_dm_group(&members)
                .map_err(XmtpError::unknown)?
            else {
                return Ok(None);
            };
            let group = client.group(&stored.id).map_err(XmtpError::unknown)?;
            Ok(Some(Arc::new(Dm::from_core(group, client_key).await?)))
        })
        .await
    }

    pub async fn create_group(
        &self,
        members: Vec<InboxID>,
        options: Option<CreateGroupOptions>,
    ) -> Result<Arc<Group>, XmtpError> {
        let members: Vec<String> = members.into_iter().map(|member| member.0).collect();
        let (permissions, metadata) = options.unwrap_or_default().into_core()?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let group = client
                .create_group_with_members(&members, permissions, Some(metadata))
                .await
                .map_err(XmtpError::unknown)?;
            Ok(Arc::new(Group::from_core(group, client_key).await?))
        })
        .await
    }

    pub async fn create_dm(
        &self,
        peer: InboxID,
        options: Option<CreateDmOptions>,
    ) -> Result<Arc<Dm>, XmtpError> {
        let client = self.client.clone();
        let metadata = options.unwrap_or_default().into();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let group = client
                .find_or_create_dm(peer.0, Some(metadata))
                .await
                .map_err(XmtpError::unknown)?;
            Ok(Arc::new(Dm::from_core(group, client_key).await?))
        })
        .await
    }

    pub async fn get_by_id(&self, id: ConversationID) -> Result<Option<Conversation>, XmtpError> {
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let id: GroupId = id.try_into()?;
            if client
                .context
                .db()
                .find_group(&id)
                .map_err(XmtpError::unknown)?
                .is_none()
            {
                return Ok(None);
            }
            let group = client.stitched_group(&id).map_err(XmtpError::unknown)?;
            Conversation::from_core(group, client_key).await
        })
        .await
    }

    pub async fn list(
        &self,
        options: Option<ListConversationsOptions>,
    ) -> Result<Vec<Conversation>, XmtpError> {
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let args: GroupQueryArgs = options.unwrap_or_default().into();
            let groups = client
                .list_conversations(args)
                .map_err(XmtpError::unknown)?;
            let mut result = Vec::with_capacity(groups.len());
            for item in groups {
                if let Some(conversation) = Conversation::from_core(item.group, client_key).await? {
                    result.push(conversation);
                }
            }
            Ok(result)
        })
        .await
    }

    pub async fn list_groups(
        &self,
        options: Option<ListConversationsOptions>,
    ) -> Result<Vec<Arc<Group>>, XmtpError> {
        let mut options = options.unwrap_or_default();
        options.kind = Some(crate::ConversationKind::Group);
        Ok(self
            .list(Some(options))
            .await?
            .into_iter()
            .filter_map(|item| match item {
                Conversation::Group { group } => Some(group),
                Conversation::Dm { .. } => None,
            })
            .collect())
    }

    pub async fn list_dms(
        &self,
        options: Option<ListConversationsOptions>,
    ) -> Result<Vec<Arc<Dm>>, XmtpError> {
        let mut options = options.unwrap_or_default();
        options.kind = Some(crate::ConversationKind::Dm);
        Ok(self
            .list(Some(options))
            .await?
            .into_iter()
            .filter_map(|item| match item {
                Conversation::Dm { dm } => Some(dm),
                Conversation::Group { .. } => None,
            })
            .collect())
    }

    pub async fn get_message_by_id(&self, id: MessageID) -> Result<Option<Message>, XmtpError> {
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let bytes = hex::decode(id.0).map_err(XmtpError::unknown)?;
            client
                .message_with_group(&bytes)
                .await
                .map_err(XmtpError::unknown)?
                .map(|(stored, group)| {
                    let enriched = xmtp_mls::messages::enrichment::enrich_messages(
                        group.context.db(),
                        &stored.group_id,
                        vec![stored.clone()],
                    )
                    .map_err(XmtpError::unknown)?;
                    if let Some(value) = enriched.into_iter().next() {
                        let parent = parent_stored(&group, &value)?;
                        Message::from_enriched(stored, value, parent, client_key)
                    } else {
                        Message::from_stored(stored, client_key)
                    }
                })
                .transpose()
        })
        .await
    }

    pub async fn delete_message_locally(&self, id: MessageID) -> Result<(), XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            client
                .delete_message(hex::decode(id.0).map_err(XmtpError::unknown)?)
                .map_err(XmtpError::unknown)?;
            Ok(())
        })
        .await
    }

    pub async fn delete_message(&self, id: MessageID) -> Result<MessageID, XmtpError> {
        let (stored, group) = self.message_group(&id).await?;
        on_sdk_worker(self.client.context.clone(), async move {
            let group = deletion_group(group, &stored)?;
            let deletion_id = group
                .delete_message(stored.id)
                .map_err(XmtpError::unknown)?;
            MessageID::from_bytes(&deletion_id)
        })
        .await
    }

    pub async fn react_to_message(
        &self,
        id: MessageID,
        reaction: Reaction,
        options: Option<SendOptions>,
    ) -> Result<MessageID, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            Box::pin(async move {
                let bytes = hex::decode(&id.0).map_err(XmtpError::unknown)?;
                let (stored, group) = client
                    .message_with_group(&bytes)
                    .await
                    .map_err(XmtpError::unknown)?
                    .ok_or_else(|| XmtpError::invalid("message not found"))?;
                let content = ReactionCodec::encode(
                    reaction.into_proto(id, InboxID::try_from(stored.sender_inbox_id)?),
                )
                .map_err(XmtpError::unknown)?;
                send_encoded(group, content.into(), options.unwrap_or_default()).await
            })
            .await
        })
        .await
    }

    pub async fn reply_to_message(
        &self,
        id: MessageID,
        content: EncodedContent,
        options: Option<SendOptions>,
    ) -> Result<MessageID, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            Box::pin(async move {
                let bytes = hex::decode(&id.0).map_err(XmtpError::unknown)?;
                let (stored, group) = client
                    .message_with_group(&bytes)
                    .await
                    .map_err(XmtpError::unknown)?
                    .ok_or_else(|| XmtpError::invalid("message not found"))?;
                let reply = Reply {
                    reference: id.0,
                    reference_inbox_id: Some(stored.sender_inbox_id),
                    content: content.into(),
                };
                let encoded = ReplyCodec::encode(reply).map_err(XmtpError::unknown)?;
                send_encoded(group, encoded.into(), options.unwrap_or_default()).await
            })
            .await
        })
        .await
    }

    pub async fn sync(&self) -> Result<(), XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            client.sync_welcomes().await.map_err(XmtpError::unknown)?;
            Ok(())
        })
        .await
    }

    pub async fn sync_all(
        &self,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<GroupSyncSummary, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            client
                .sync_all_welcomes_and_groups(
                    consent_states.map(|states| states.into_iter().map(Into::into).collect()),
                )
                .await
                .map(Into::into)
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn hmac_keys(&self) -> Result<Vec<ConversationHmacKeys>, XmtpError> {
        let client = self.client.clone();
        on_sdk_worker(self.client.context.clone(), async move {
            let mut groups = client
                .find_groups(GroupQueryArgs {
                    include_duplicate_dms: true,
                    ..Default::default()
                })
                .map_err(XmtpError::unknown)?;
            let mut entries = Vec::with_capacity(groups.len());
            for group in groups.drain(..) {
                entries.push(ConversationHmacKeys {
                    conversation_id: group.group_id.into(),
                    keys: group
                        .hmac_keys(-1..=1)
                        .map_err(XmtpError::unknown)?
                        .into_iter()
                        .map(Into::into)
                        .collect(),
                });
            }
            Ok(entries)
        })
        .await
    }
}

impl Conversations {
    async fn message_group(
        &self,
        id: &MessageID,
    ) -> Result<
        (
            xmtp_db::group_message::StoredGroupMessage,
            MlsGroup<xmtp_mls::MlsContext>,
        ),
        XmtpError,
    > {
        let client = self.client.clone();
        let bytes = hex::decode(&id.0).map_err(XmtpError::unknown)?;
        on_sdk_worker(self.client.context.clone(), async move {
            client
                .message_with_group(&bytes)
                .await
                .map_err(XmtpError::unknown)?
                .ok_or_else(|| XmtpError::invalid("message not found"))
        })
        .await
    }
}

#[cfg(feature = "bench")]
#[xmtp_macro::sdk_export]
impl Conversations {
    /// Open a group already stored in this client's database for the benchmark.
    pub async fn get_group(&self, id: ConversationID) -> Result<Arc<Group>, XmtpError> {
        let bytes = hex::decode(id.0).map_err(XmtpError::unknown)?;
        let group_id = xmtp_proto::types::GroupId::try_from(bytes).map_err(XmtpError::unknown)?;
        let client = self.client.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.client.context.clone(), async move {
            let group = client.group(&group_id).map_err(XmtpError::unknown)?;
            Ok(Arc::new(Group::from_core(group, client_key).await?))
        })
        .await
    }
}

#[derive(uniffi::Object)]
pub struct Group {
    pub(crate) inner: MlsGroup<xmtp_mls::MlsContext>,
    pub(crate) client_key: u64,
    identity: ConversationIdentity,
    #[cfg(test)]
    pub(crate) state_counts: Arc<parking_lot::Mutex<(u64, u64, u64)>>,
}

#[derive(uniffi::Object)]
pub struct Dm {
    pub(crate) inner: MlsGroup<xmtp_mls::MlsContext>,
    pub(crate) client_key: u64,
    identity: ConversationIdentity,
    peer_inbox_id: InboxID,
    #[cfg(test)]
    pub(crate) state_counts: Arc<parking_lot::Mutex<(u64, u64, u64)>>,
}

struct ConversationIdentity {
    added_by_inbox_id: InboxID,
    creator_inbox_id: InboxID,
    is_creator: bool,
}

impl ConversationIdentity {
    async fn from_core(
        group: &MlsGroup<xmtp_mls::MlsContext>,
    ) -> Result<(Self, xmtp_mls::mls_common::group_metadata::GroupMetadata), XmtpError> {
        let added_by_inbox_id =
            InboxID::try_from(group.added_by_inbox_id().map_err(XmtpError::unknown)?)?;
        let metadata = group.metadata().await.map_err(XmtpError::unknown)?;
        let is_creator = metadata.creator_inbox_id == group.context.inbox_id();
        Ok((
            Self {
                added_by_inbox_id,
                creator_inbox_id: InboxID::try_from(metadata.creator_inbox_id.clone())?,
                is_creator,
            },
            metadata,
        ))
    }
}

impl Group {
    pub(crate) async fn from_core(
        inner: MlsGroup<xmtp_mls::MlsContext>,
        client_key: u64,
    ) -> Result<Self, XmtpError> {
        let (identity, _) = ConversationIdentity::from_core(&inner).await?;
        Ok(Self {
            inner,
            client_key,
            identity,
            #[cfg(test)]
            state_counts: Arc::new(parking_lot::Mutex::new((0, 0, 0))),
        })
    }
}

impl Dm {
    async fn from_core(
        inner: MlsGroup<xmtp_mls::MlsContext>,
        client_key: u64,
    ) -> Result<Self, XmtpError> {
        let (identity, metadata) = ConversationIdentity::from_core(&inner).await?;
        let members = metadata
            .dm_members
            .ok_or_else(|| XmtpError::invalid("DM has no peer metadata"))?;
        let peer = if members.member_one_inbox_id == inner.context.inbox_id() {
            members.member_two_inbox_id
        } else {
            members.member_one_inbox_id
        };
        let peer_inbox_id = InboxID::try_from(peer.to_string())?;
        Ok(Self {
            inner,
            client_key,
            identity,
            peer_inbox_id,
            #[cfg(test)]
            state_counts: Arc::new(parking_lot::Mutex::new((0, 0, 0))),
        })
    }
}

async fn send_standard(
    group: MlsGroup<xmtp_mls::MlsContext>,
    value: StandardContent,
    options: Option<SendOptions>,
) -> Result<MessageID, XmtpError> {
    let default_push = !matches!(
        value,
        StandardContent::Reaction { .. } | StandardContent::ReadReceipt
    );
    let options = options.unwrap_or(SendOptions {
        should_push: default_push,
        ..SendOptions::default()
    });
    send_encoded(group, crate::encode_standard(value)?, options).await
}

async fn send_encoded(
    group: MlsGroup<xmtp_mls::MlsContext>,
    content: EncodedContent,
    options: SendOptions,
) -> Result<MessageID, XmtpError> {
    on_sdk_worker(group.context.clone(), async move {
        // Build the send future on the worker. Swift cooperative threads have
        // a small stack and cannot hold this nested MLS future before spawn.
        Box::pin(async move {
            let content =
                compress_if_requested(content.into(), options.compression.map(Into::into))
                    .map_err(XmtpError::unknown)?;
            let bytes = encoded_content_to_bytes(content);
            let opts = SendMessageOpts {
                should_push: options.should_push,
                idempotency_key: options.idempotency_key,
            };
            let id = if options.optimistic {
                group
                    .send_message_optimistic(&bytes, opts)
                    .map_err(XmtpError::unknown)?
            } else {
                group
                    .send_message(&bytes, opts)
                    .await
                    .map_err(XmtpError::unknown)?
            };
            MessageID::from_bytes(&id)
        })
        .await
    })
    .await
}

fn parent_stored(
    group: &MlsGroup<xmtp_mls::MlsContext>,
    message: &DecodedMessage,
) -> Result<Option<StoredGroupMessage>, XmtpError> {
    let CoreMessageBody::Reply(reply) = &message.content else {
        return Ok(None);
    };
    let Some(parent) = &reply.in_reply_to else {
        return Ok(None);
    };
    group
        .context
        .db()
        .get_group_message(&parent.metadata.id)
        .map_err(XmtpError::unknown)
}

pub(crate) fn query_content_types(
    values: Vec<ContentTypeId>,
) -> Result<Vec<xmtp_db::group_message::ContentType>, XmtpError> {
    use xmtp_db::group_message::ContentType as C;
    values
        .into_iter()
        .map(|value| {
            if value.authority_id != "xmtp.org" {
                return Err(XmtpError::invalid("unsupported content type authority"));
            }
            Ok(match value.type_id.as_str() {
                "text" => C::Text,
                "markdown" => C::Markdown,
                "reaction" => C::Reaction,
                "reply" => C::Reply,
                "attachment" => C::Attachment,
                "remoteAttachment" => C::RemoteAttachment,
                "multiRemoteAttachment" => C::MultiRemoteAttachment,
                "readReceipt" => C::ReadReceipt,
                "groupUpdated" => C::GroupUpdated,
                "transactionReference" => C::TransactionReference,
                "walletSendCalls" => C::WalletSendCalls,
                "actions" => C::Actions,
                "intent" => C::Intent,
                "leaveRequest" => C::LeaveRequest,
                "deleteMessage" => C::DeleteMessage,
                _ => C::Unknown,
            })
        })
        .collect()
}

// One block defines the shared Group and Dm methods.
macro_rules! common_conversation {
    ($name:ident, $state:ty, $map:expr) => {
        #[xmtp_macro::sdk_export]
        impl $name {
            pub fn id(&self) -> ConversationID {
                self.inner.group_id.into()
            }

            pub fn created_at(&self) -> Timestamp {
                Timestamp(self.inner.created_at_ns)
            }

            pub fn topic(&self) -> String {
                xmtp_proto::types::Topic::new_group_message(self.inner.group_id).to_string()
            }

            pub fn kind(&self) -> crate::ConversationKind {
                match self.inner.conversation_type {
                    ConversationType::Dm => crate::ConversationKind::Dm,
                    _ => crate::ConversationKind::Group,
                }
            }

            pub fn added_by_inbox_id(&self) -> InboxID {
                self.identity.added_by_inbox_id.clone()
            }

            pub fn creator_inbox_id(&self) -> InboxID {
                self.identity.creator_inbox_id.clone()
            }

            pub fn is_creator(&self) -> bool {
                self.identity.is_creator
            }

            pub async fn state(&self) -> Result<$state, XmtpError> {
                let group = self.inner.clone();
                #[cfg(test)]
                let counts = self.state_counts.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    #[cfg(test)]
                    {
                        let ((state, key_reads), queries, writes) =
                            xmtp_db::count_sql_queries(|| {
                                xmtp_db::sql_key_store::count_kv_reads(|| {
                                    let snapshot =
                                        group.state_snapshot().map_err(XmtpError::unknown)?;
                                    ($map)(snapshot)
                                })
                            });
                        *counts.lock() = (queries.saturating_sub(key_reads), key_reads, writes);
                        return state;
                    }
                    #[cfg(not(test))]
                    {
                        let snapshot = group.state_snapshot().map_err(XmtpError::unknown)?;
                        ($map)(snapshot)
                    }
                })
                .await
            }

            pub async fn last_activity_at_ns(
                &self,
                content_types: Option<Vec<ContentTypeId>>,
            ) -> Result<Timestamp, XmtpError> {
                let types = content_types.map(query_content_types).transpose()?;
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .last_activity_ns(types.as_deref())
                        .map(Timestamp)
                        .map_err(XmtpError::unknown)
                })
                .await
            }

            pub async fn update_consent_state(&self, state: ConsentState) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .update_consent_state(state.into())
                        .map_err(XmtpError::unknown)
                })
                .await
            }

            pub async fn sync(&self) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group.sync().await.map_err(XmtpError::unknown)?;
                    Ok(())
                })
                .await
            }

            pub async fn members(&self) -> Result<Vec<Member>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .members()
                        .await
                        .map_err(XmtpError::unknown)?
                        .into_iter()
                        .map(Member::try_from)
                        .collect()
                })
                .await
            }

            pub async fn debug_info(&self) -> Result<crate::ConversationDebugInfo, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .debug_info()
                        .await
                        .map(Into::into)
                        .map_err(XmtpError::unknown)
                })
                .await
            }

            pub async fn hmac_keys(&self) -> Result<Vec<HmacKey>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    Ok(group
                        .hmac_keys(-1..=1)
                        .map_err(XmtpError::unknown)?
                        .into_iter()
                        .map(Into::into)
                        .collect())
                })
                .await
            }

            pub async fn last_read_times(&self) -> Result<Vec<LastReadTimeEntry>, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .get_last_read_times()
                        .map_err(XmtpError::unknown)?
                        .into_iter()
                        .map(|(inbox_id, ns)| {
                            Ok(LastReadTimeEntry {
                                inbox_id: InboxID::try_from(inbox_id)?,
                                read_at: Timestamp(ns),
                            })
                        })
                        .collect()
                })
                .await
            }

            pub async fn update_disappearing_settings(
                &self,
                settings: Option<DisappearingSettings>,
            ) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    match settings {
                        Some(settings) => {
                            group
                                .update_conversation_message_disappearing_settings(settings.into())
                                .await
                        }
                        None => {
                            group
                                .remove_conversation_message_disappearing_settings()
                                .await
                        }
                    }
                    .map_err(XmtpError::unknown)
                })
                .await
            }

            pub async fn set_notifications(
                &self,
                value: NotificationOverride,
            ) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .set_notifications(value.into())
                        .map_err(XmtpError::unknown)
                })
                .await
            }

            pub async fn publish_messages(&self) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group.publish_messages().await.map_err(XmtpError::unknown)
                })
                .await
            }

            pub async fn publish_message(&self, id: MessageID) -> Result<(), XmtpError> {
                let group = self.inner.clone();
                let bytes = hex::decode(id.0).map_err(XmtpError::unknown)?;
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .publish_stored_message(&bytes)
                        .await
                        .map_err(XmtpError::unknown)
                })
                .await
            }

            pub async fn prepare_message(
                &self,
                encoded: EncodedContent,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                let mut options = options.unwrap_or_default();
                options.optimistic = true;
                send_encoded(self.inner.clone(), encoded, options).await
            }

            pub async fn send(
                &self,
                encoded: EncodedContent,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_encoded(self.inner.clone(), encoded, options.unwrap_or_default()).await
            }

            pub async fn send_text(
                &self,
                text: String,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(self.inner.clone(), StandardContent::Text(text), options).await
            }

            pub async fn send_markdown(
                &self,
                markdown: String,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Markdown(markdown),
                    options,
                )
                .await
            }

            pub async fn send_reaction(
                &self,
                reference: MessageID,
                reference_inbox_id: Option<InboxID>,
                reaction: Reaction,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Reaction {
                        reference,
                        reference_inbox_id,
                        reaction,
                    },
                    options,
                )
                .await
            }

            pub async fn send_reply(
                &self,
                reference: MessageID,
                reference_inbox_id: Option<InboxID>,
                content: EncodedContent,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Reply {
                        reference,
                        reference_inbox_id,
                        content,
                    },
                    options,
                )
                .await
            }

            pub async fn send_read_receipt(
                &self,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(self.inner.clone(), StandardContent::ReadReceipt, options).await
            }

            pub async fn send_attachment(
                &self,
                attachment: crate::Attachment,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Attachment(attachment),
                    options,
                )
                .await
            }

            pub async fn send_remote_attachment(
                &self,
                attachment: crate::RemoteAttachment,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::RemoteAttachment(attachment),
                    options,
                )
                .await
            }

            pub async fn send_multi_remote_attachment(
                &self,
                attachment: crate::MultiRemoteAttachment,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::MultiRemoteAttachment(attachment),
                    options,
                )
                .await
            }

            pub async fn send_transaction_reference(
                &self,
                reference: crate::TransactionReference,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::TransactionReference(reference),
                    options,
                )
                .await
            }

            pub async fn send_wallet_send_calls(
                &self,
                calls: crate::WalletSendCalls,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::WalletSendCalls(calls),
                    options,
                )
                .await
            }

            pub async fn send_actions(
                &self,
                actions: crate::Actions,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(
                    self.inner.clone(),
                    StandardContent::Actions(actions),
                    options,
                )
                .await
            }

            pub async fn send_intent(
                &self,
                intent: crate::Intent,
                options: Option<SendOptions>,
            ) -> Result<MessageID, XmtpError> {
                send_standard(self.inner.clone(), StandardContent::Intent(intent), options).await
            }

            pub async fn messages(
                &self,
                options: Option<ListMessagesOptions>,
            ) -> Result<Vec<Message>, XmtpError> {
                let query: MsgQueryArgs = options.unwrap_or_default().try_into()?;
                let group = self.inner.clone();
                let client_key = self.client_key;
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .find_messages_v2(&query)
                        .map_err(XmtpError::unknown)?
                        .into_iter()
                        .map(|enriched| {
                            let stored = group
                                .context
                                .db()
                                .get_group_message(&enriched.metadata.id)
                                .map_err(XmtpError::unknown)?
                                .ok_or_else(|| XmtpError::invalid("message not found"))?;
                            let parent = parent_stored(&group, &enriched)?;
                            Message::from_enriched(stored, enriched, parent, client_key)
                        })
                        .collect()
                })
                .await
            }

            pub async fn count_messages(
                &self,
                options: Option<ListMessagesOptions>,
            ) -> Result<u64, XmtpError> {
                let query: MsgQueryArgs = options.unwrap_or_default().try_into()?;
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    group
                        .count_messages(&query)
                        .map(|count| count as u64)
                        .map_err(XmtpError::unknown)
                })
                .await
            }

            pub async fn last_message(&self) -> Result<Option<Message>, XmtpError> {
                Ok(self
                    .messages(Some(ListMessagesOptions {
                        limit: Some(1),
                        direction: Some(crate::MessageOrder::Descending),
                        ..Default::default()
                    }))
                    .await?
                    .into_iter()
                    .next())
            }

            pub async fn delete_message(&self, id: MessageID) -> Result<MessageID, XmtpError> {
                let group = self.inner.clone();
                on_sdk_worker(self.inner.context.clone(), async move {
                    let bytes = hex::decode(&id.0).map_err(XmtpError::unknown)?;
                    let stored = group
                        .context
                        .db()
                        .get_group_message(&bytes)
                        .map_err(XmtpError::unknown)?
                        .ok_or_else(|| XmtpError::invalid("message not found"))?;
                    let group = deletion_group(group, &stored)?;
                    let deletion_id = group.delete_message(bytes).map_err(XmtpError::unknown)?;
                    MessageID::from_bytes(&deletion_id)
                })
                .await
            }

            pub async fn message_reader(&self) -> Result<Arc<MessageReader>, XmtpError> {
                let context = self.inner.context.clone();
                let group_id = self.inner.group_id;
                let client_key = self.client_key;
                on_sdk_worker(self.inner.context.clone(), async move {
                    MessageReader::open(context, group_id, client_key)
                })
                .await
            }
        }
    };
}

common_conversation!(Group, GroupState, GroupState::from_snapshot);
common_conversation!(Dm, ConversationState, |snapshot| Ok(
    ConversationState::from_snapshot(&snapshot)
));

#[xmtp_macro::sdk_export]
impl Group {
    pub async fn peer_inbox_ids(&self) -> Result<Vec<InboxID>, XmtpError> {
        let own = self.inner.context.inbox_id().to_string();
        Ok(self
            .members()
            .await?
            .into_iter()
            .map(|member| member.inbox_id)
            .filter(|id| id.0 != own)
            .collect())
    }

    pub async fn update_name(&self, value: String) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .update_group_name(value)
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn update_description(&self, value: String) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .update_group_description(value)
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn update_image_url(&self, value: String) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .update_group_image_url_square(value)
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn update_app_data(
        &self,
        value: String,
        expected: Option<String>,
    ) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .update_app_data(value, expected)
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn update_permission(
        &self,
        kind: crate::PermissionUpdateKind,
        policy: crate::PermissionPolicy,
        metadata_field: Option<crate::MetadataFieldKind>,
    ) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        let policy = policy.try_into()?;
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .update_permission_policy(kind.into(), policy, metadata_field.map(Into::into))
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn add_members(
        &self,
        members: Vec<InboxID>,
    ) -> Result<crate::MembershipResult, XmtpError> {
        let group = self.inner.clone();
        let ids = members.into_iter().map(|id| id.0).collect::<Vec<_>>();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .add_members(&ids)
                .await
                .map_err(XmtpError::unknown)?
                .try_into()
        })
        .await
    }

    pub async fn remove_members(&self, members: Vec<InboxID>) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        let ids = members.into_iter().map(|id| id.0).collect::<Vec<_>>();
        on_sdk_worker(self.inner.context.clone(), async move {
            let refs = ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
            group
                .remove_members(&refs)
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn add_admin(&self, inbox_id: InboxID) -> Result<(), XmtpError> {
        self.update_admin_list(xmtp_mls::groups::UpdateAdminListType::Add, inbox_id)
            .await
    }

    pub async fn remove_admin(&self, inbox_id: InboxID) -> Result<(), XmtpError> {
        self.update_admin_list(xmtp_mls::groups::UpdateAdminListType::Remove, inbox_id)
            .await
    }

    pub async fn add_super_admin(&self, inbox_id: InboxID) -> Result<(), XmtpError> {
        self.update_admin_list(xmtp_mls::groups::UpdateAdminListType::AddSuper, inbox_id)
            .await
    }

    pub async fn remove_super_admin(&self, inbox_id: InboxID) -> Result<(), XmtpError> {
        self.update_admin_list(xmtp_mls::groups::UpdateAdminListType::RemoveSuper, inbox_id)
            .await
    }

    pub async fn is_admin(&self, inbox_id: InboxID) -> Result<bool, XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group.is_admin(inbox_id.0).map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn is_super_admin(&self, inbox_id: InboxID) -> Result<bool, XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group.is_super_admin(inbox_id.0).map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn list_admins(&self) -> Result<Vec<InboxID>, XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .admin_list()
                .map_err(XmtpError::unknown)?
                .into_iter()
                .map(InboxID::try_from)
                .collect()
        })
        .await
    }

    pub async fn list_super_admins(&self) -> Result<Vec<InboxID>, XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .super_admin_list()
                .map_err(XmtpError::unknown)?
                .into_iter()
                .map(InboxID::try_from)
                .collect()
        })
        .await
    }

    pub async fn request_removal(&self) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group.leave_group().await.map_err(XmtpError::unknown)
        })
        .await
    }

    pub async fn membership_capabilities(
        &self,
    ) -> Result<crate::GroupMembershipCapabilities, XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .membership_capabilities()
                .await
                .map_err(XmtpError::unknown)?
                .try_into()
        })
        .await
    }
}

impl Group {
    async fn update_admin_list(
        &self,
        action: xmtp_mls::groups::UpdateAdminListType,
        inbox_id: InboxID,
    ) -> Result<(), XmtpError> {
        let group = self.inner.clone();
        on_sdk_worker(self.inner.context.clone(), async move {
            group
                .update_admin_list(action, inbox_id.0)
                .await
                .map_err(XmtpError::unknown)
        })
        .await
    }
}

#[xmtp_macro::sdk_export]
impl Dm {
    pub fn peer_inbox_id(&self) -> InboxID {
        self.peer_inbox_id.clone()
    }

    pub async fn duplicate_dms(&self) -> Result<Vec<Arc<Dm>>, XmtpError> {
        let group = self.inner.clone();
        let client_key = self.client_key;
        on_sdk_worker(self.inner.context.clone(), async move {
            let groups = group.find_duplicate_dms().map_err(XmtpError::unknown)?;
            let mut result = Vec::with_capacity(groups.len());
            for inner in groups {
                result.push(Arc::new(Dm::from_core(inner, client_key).await?));
            }
            Ok(result)
        })
        .await
    }
}
