//! The conversations collection and its streaming entry points.

use super::*;

use crate::identity::FfiIdentifier;
use crate::message::FfiDecodedMessage;
use crate::{FfiError, GenericError};
use std::{collections::HashMap, convert::TryInto, sync::Arc};

use xmtp_db::group::ConversationType;
use xmtp_db::{consent_record::ConsentState, group::GroupQueryArgs};
use xmtp_mls::subscriptions::local_delivery::{DeliveryScope, LocalDeliveryFilter};
use xmtp_mls::worker::device_sync::preference_sync::PreferenceUpdate;

#[derive(uniffi::Record, Default)]
pub struct FfiListConversationsOptions {
    pub created_after_ns: Option<i64>,
    pub created_before_ns: Option<i64>,
    pub last_activity_before_ns: Option<i64>,
    pub last_activity_after_ns: Option<i64>,
    pub order_by: Option<FfiGroupQueryOrderBy>,
    pub limit: Option<i64>,
    pub consent_states: Option<Vec<FfiConsentState>>,
    pub include_duplicate_dms: bool,
}

impl From<FfiListConversationsOptions> for GroupQueryArgs {
    fn from(opts: FfiListConversationsOptions) -> GroupQueryArgs {
        GroupQueryArgs {
            created_before_ns: opts.created_before_ns,
            created_after_ns: opts.created_after_ns,
            limit: opts.limit,
            consent_states: opts
                .consent_states
                .map(|vec| vec.into_iter().map(Into::into).collect()),
            include_duplicate_dms: opts.include_duplicate_dms,
            last_activity_before_ns: opts.last_activity_before_ns,
            last_activity_after_ns: opts.last_activity_after_ns,
            order_by: opts.order_by.map(Into::into),
            ..Default::default()
        }
    }
}

#[derive(uniffi::Object)]
pub struct FfiConversations {
    pub(crate) inner_client: Arc<RustXmtpClient>,
}

impl FfiConversations {
    /// Route every mobile conversation stream through shared bidi receipt.
    /// Keep Rust-only callback types outside the exported UniFFI implementation.
    fn stream_conversations_dispatch(
        &self,
        conversation_type: Option<ConversationType>,
        callback: Arc<dyn FfiConversationCallback>,
    ) -> FfiStreamCloser {
        let close_cb = callback.clone();
        FfiStreamCloser::new(RustXmtpClient::stream_conversations_with_callback_dispatch(
            self.inner_client.clone(),
            conversation_type,
            false,
            move |convo| match convo {
                Ok(c) => callback.on_conversation(Arc::new(c.into())),
                Err(e) => callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
        ))
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversations {
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn create_group_optimistic(
        &self,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, FfiError> {
        log::info!("creating optimistic group");

        if let Some(FfiGroupPermissionsOptions::CustomPolicy) = opts.permissions {
            if opts.custom_permission_policy_set.is_none() {
                return Err(FfiError::generic("CustomPolicy must include policy set"));
            }
        } else if opts.custom_permission_policy_set.is_some() {
            return Err(FfiError::generic(
                "Only CustomPolicy may specify a policy set",
            ));
        }

        let metadata_options = opts.clone().into_group_metadata_options();

        let group_permissions = match opts.permissions {
            Some(FfiGroupPermissionsOptions::Default) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::Default.to_policy_set())
            }
            Some(FfiGroupPermissionsOptions::AdminOnly) => {
                Some(xmtp_mls::groups::PreconfiguredPolicies::AdminsOnly.to_policy_set())
            }
            Some(FfiGroupPermissionsOptions::CustomPolicy) => {
                if let Some(policy_set) = opts.custom_permission_policy_set {
                    Some(policy_set.try_into()?)
                } else {
                    None
                }
            }
            _ => None,
        };

        let convo = self
            .inner_client
            .create_group(group_permissions, Some(metadata_options))?;

        Ok(Arc::new(convo.into()))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn create_group_by_identity(
        &self,
        account_identities: Vec<FfiIdentifier>,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, FfiError> {
        log::info!(
            "creating group with account addresses: {}",
            account_identities
                .iter()
                .map(|ident| format!("{ident}"))
                .collect::<Vec<_>>()
                .join(", ")
        );

        let convo = self.create_group_optimistic(opts)?;

        if !account_identities.is_empty() {
            convo.add_members_by_identity(account_identities).await?;
        } else {
            convo.sync().await?;
        }

        Ok(convo)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn create_group(
        &self,
        inbox_ids: Vec<String>,
        opts: FfiCreateGroupOptions,
    ) -> Result<Arc<FfiConversation>, FfiError> {
        log::info!(
            "creating group with account inbox ids: {}",
            inbox_ids.join(", ")
        );

        let convo = self.create_group_optimistic(opts)?;

        if !inbox_ids.is_empty() {
            convo.add_members(inbox_ids).await?;
        } else {
            convo.sync().await?;
        };

        Ok(convo)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn find_or_create_dm_by_identity(
        &self,
        target_identity: FfiIdentifier,
        opts: FfiCreateDMOptions,
    ) -> Result<Arc<FfiConversation>, FfiError> {
        let target_identity = target_identity.try_into()?;
        log::info!("creating dm with target address: {target_identity:?}",);
        self.inner_client
            .find_or_create_dm_by_identity(target_identity, Some(opts.into_dm_metadata_options()))
            .await
            .map(|g| Arc::new(g.into()))
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn find_or_create_dm(
        &self,
        inbox_id: String,
        opts: FfiCreateDMOptions,
    ) -> Result<Arc<FfiConversation>, FfiError> {
        log::info!("creating dm with target inbox_id: {}", inbox_id);
        self.inner_client
            .find_or_create_dm(inbox_id, Some(opts.into_dm_metadata_options()))
            .await
            .map(|g| Arc::new(g.into()))
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn process_streamed_welcome_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Vec<Arc<FfiConversation>>, FfiError> {
        self.inner_client
            .process_streamed_welcome_message(envelope_bytes)
            .await
            .map(|list| list.into_iter().map(|g| Arc::new(g.into())).collect())
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync(&self) -> Result<(), FfiError> {
        let inner = self.inner_client.as_ref();
        inner.sync_welcomes().await?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync_all_conversations(
        &self,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> Result<FfiGroupSyncSummary, FfiError> {
        let inner = self.inner_client.as_ref();
        let consents: Option<Vec<ConsentState>> =
            consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());
        let summary = inner.sync_all_welcomes_and_groups(consents).await?;

        Ok(summary.into())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn list(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, FfiError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiConversationListItem>> = inner
            .list_conversations(opts.into())?
            .into_iter()
            .map(|conversation_item| {
                Arc::new(FfiConversationListItem {
                    conversation: conversation_item.group.into(),
                    last_message: conversation_item
                        .last_message
                        .map(|stored_message| stored_message.into()),
                    is_commit_log_forked: conversation_item.is_commit_log_forked,
                })
            })
            .collect();

        Ok(convo_list)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn list_groups(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, FfiError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiConversationListItem>> = inner
            .list_conversations(GroupQueryArgs {
                conversation_type: Some(ConversationType::Group),
                ..GroupQueryArgs::from(opts)
            })?
            .into_iter()
            .map(|conversation_item| {
                Arc::new(FfiConversationListItem {
                    conversation: conversation_item.group.into(),
                    last_message: conversation_item
                        .last_message
                        .map(|stored_message| stored_message.into()),
                    is_commit_log_forked: conversation_item.is_commit_log_forked,
                })
            })
            .collect();

        Ok(convo_list)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn list_dms(
        &self,
        opts: FfiListConversationsOptions,
    ) -> Result<Vec<Arc<FfiConversationListItem>>, FfiError> {
        let inner = self.inner_client.as_ref();
        let convo_list: Vec<Arc<FfiConversationListItem>> = inner
            .list_conversations(GroupQueryArgs {
                conversation_type: Some(ConversationType::Dm),
                ..GroupQueryArgs::from(opts)
            })?
            .into_iter()
            .map(|conversation_item| {
                Arc::new(FfiConversationListItem {
                    conversation: conversation_item.group.into(),
                    last_message: conversation_item
                        .last_message
                        .map(|stored_message| stored_message.into()),
                    is_commit_log_forked: conversation_item.is_commit_log_forked,
                })
            })
            .collect();

        Ok(convo_list)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn stream_groups(
        &self,
        callback: Arc<dyn FfiConversationCallback>,
    ) -> FfiStreamCloser {
        self.stream_conversations_dispatch(Some(ConversationType::Group), callback)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn stream_dms(&self, callback: Arc<dyn FfiConversationCallback>) -> FfiStreamCloser {
        self.stream_conversations_dispatch(Some(ConversationType::Dm), callback)
    }

    pub async fn stream(&self, callback: Arc<dyn FfiConversationCallback>) -> FfiStreamCloser {
        self.stream_conversations_dispatch(None, callback)
    }

    pub async fn stream_all_group_messages(
        &self,
        message_callback: Arc<dyn FfiMessageCallback>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> FfiStreamCloser {
        self.stream_messages(
            message_callback,
            Some(FfiConversationType::Group),
            consent_states,
        )
        .await
    }

    pub async fn stream_all_dm_messages(
        &self,
        message_callback: Arc<dyn FfiMessageCallback>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> FfiStreamCloser {
        self.stream_messages(
            message_callback,
            Some(FfiConversationType::Dm),
            consent_states,
        )
        .await
    }

    pub async fn stream_all_messages(
        &self,
        message_callback: Arc<dyn FfiMessageCallback>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> FfiStreamCloser {
        self.stream_messages(message_callback, None, consent_states)
            .await
    }

    /// Open the default consumer, or an independent replay reader when `from` is set.
    pub async fn message_reader(
        &self,
        group_ids: Option<Vec<Vec<u8>>>,
        conversation_type: Option<FfiConversationType>,
        consent_states: Option<Vec<FfiConsentState>>,
        from: Option<FfiDeliveryCursor>,
    ) -> Result<Arc<FfiMessageReader>, FfiError> {
        FfiMessageReader::open(
            self.inner_client.context.clone(),
            local_delivery::delivery_scope(group_ids)?,
            local_delivery::delivery_filter(conversation_type, consent_states),
            from,
        )
    }

    /// Read retained history and its stream boundary in one database snapshot.
    pub fn message_history_snapshot(
        &self,
        group_ids: Option<Vec<Vec<u8>>>,
        conversation_type: Option<FfiConversationType>,
        consent_states: Option<Vec<FfiConsentState>>,
        limit: u32,
    ) -> Result<FfiMessageHistorySnapshot, FfiError> {
        local_delivery::history_snapshot(
            &self.inner_client.context,
            local_delivery::delivery_scope(group_ids)?,
            local_delivery::delivery_filter(conversation_type, consent_states),
            limit,
        )
    }

    /// A database-bound replay cursor before the first retained delivery.
    pub fn beginning_delivery_cursor(&self) -> Result<FfiDeliveryCursor, FfiError> {
        local_delivery::beginning_cursor(&self.inner_client.context)
    }

    async fn stream_messages(
        &self,
        message_callback: Arc<dyn FfiMessageCallback>,
        conversation_type: Option<FfiConversationType>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> FfiStreamCloser {
        let consents: Option<Vec<ConsentState>> =
            consent_states.map(|states| states.into_iter().map(|state| state.into()).collect());
        local_delivery::stream_messages(
            self.inner_client.context.clone(),
            DeliveryScope::All,
            LocalDeliveryFilter {
                conversation_type: conversation_type.map(Into::into),
                consent_states: consents,
            },
            message_callback,
        )
    }

    /// Get notified when there is a new consent update either locally or is synced from another device
    /// allowing the user to re-render the new state appropriately
    pub async fn stream_consent(&self, callback: Arc<dyn FfiConsentCallback>) -> FfiStreamCloser {
        let close_cb = callback.clone();
        let handle = RustXmtpClient::stream_consent_with_callback(
            self.inner_client.clone(),
            move |msg| match msg {
                Ok(m) => callback.on_consent_update(m.into_iter().map(Into::into).collect()),
                Err(e) => callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
        );

        FfiStreamCloser::new(handle)
    }

    /// Get notified when a preference changes either locally or is synced from another device
    /// allowing the user to re-render the new state appropriately.
    pub async fn stream_preferences(
        &self,
        callback: Arc<dyn FfiPreferenceCallback>,
    ) -> FfiStreamCloser {
        let close_cb = callback.clone();
        let handle = RustXmtpClient::stream_preferences_with_callback(
            self.inner_client.clone(),
            move |msg| match msg {
                Ok(m) => callback.on_preference_update(
                    m.into_iter().filter_map(|v| v.try_into().ok()).collect(),
                ),
                Err(e) => callback.on_error(e.into()),
            },
            move || close_cb.on_close(),
        );

        FfiStreamCloser::new(handle)
    }

    /// Get notified when a message is deleted by the disappearing messages worker.
    /// The callback receives the decoded message that was deleted.
    pub async fn stream_message_deletions(
        &self,
        callback: Arc<dyn FfiMessageDeletionCallback>,
    ) -> FfiStreamCloser {
        let handle = RustXmtpClient::stream_message_deletions_with_callback(
            self.inner_client.clone(),
            move |msg| {
                if let Ok(message) = msg {
                    let ffi_message: FfiDecodedMessage = message.into();
                    callback.on_message_deleted(Arc::new(ffi_message))
                }
            },
            || {},
        );

        FfiStreamCloser::new(handle)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn get_hmac_keys(&self) -> Result<HashMap<Vec<u8>, Vec<FfiHmacKey>>, FfiError> {
        let inner = self.inner_client.as_ref();
        let conversations = inner.find_groups(GroupQueryArgs {
            include_duplicate_dms: true,
            ..GroupQueryArgs::default()
        })?;

        let mut hmac_map = HashMap::new();
        for conversation in conversations {
            let id = conversation.group_id.to_vec();
            let keys = conversation
                .hmac_keys(-1..=1)?
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>();

            hmac_map.insert(id, keys);
        }

        Ok(hmac_map)
    }
}

#[cfg(test)]
impl FfiConversations {
    pub async fn get_sync_group(&self) -> Result<FfiConversation, FfiError> {
        let inner = self.inner_client.as_ref();
        let sync_group = inner.device_sync_client().get_sync_group().await?;
        Ok(sync_group.into())
    }
}

impl From<FfiConversationType> for ConversationType {
    fn from(value: FfiConversationType) -> Self {
        match value {
            FfiConversationType::Dm => ConversationType::Dm,
            FfiConversationType::Group => ConversationType::Group,
            FfiConversationType::Sync => ConversationType::Sync,
            FfiConversationType::Oneshot => ConversationType::Oneshot,
        }
    }
}

impl TryFrom<PreferenceUpdate> for FfiPreferenceUpdate {
    type Error = GenericError;
    fn try_from(value: PreferenceUpdate) -> Result<Self, Self::Error> {
        match value {
            PreferenceUpdate::Hmac { key, .. } => Ok(FfiPreferenceUpdate::HMAC { key }),
            // These are filtered out in the stream and should not be here
            // We're keeping preference update and consent streams separate right now.
            PreferenceUpdate::Consent(_) => Err(GenericError::Generic {
                err: "Consent updates should be filtered out.".to_string(),
            }),
        }
    }
}
