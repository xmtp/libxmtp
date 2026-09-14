//! A single conversation, stream closers, and callbacks.

use super::*;

use crate::FfiError;
use crate::identity::{FfiCollectionExt, FfiCollectionTryExt, FfiIdentifier};
use crate::message::FfiDecodedMessage;

use std::{collections::HashMap, convert::TryInto, sync::Arc};
use tokio::sync::Mutex;
use xmtp_common::{AbortHandle, GenericStreamHandle, StreamHandle};
use xmtp_content_types::text::TextCodec;
use xmtp_content_types::{ContentCodec, encoded_content_to_bytes};
use xmtp_db::group::DmIdExt;

use xmtp_mls::context::XmtpSharedContext;
use xmtp_mls::mls_common::group::DMMetadataOptions;
use xmtp_mls::mls_common::group::GroupMetadataOptions;
use xmtp_mls::mls_common::group_mutable_metadata::MessageDisappearingSettings;
use xmtp_mls::mls_common::group_mutable_metadata::MetadataField;
use xmtp_mls::subscriptions::{
    local_delivery::{DeliveryScope, LocalDeliveryFilter},
    message_reader::MessageReaderControl,
};
use xmtp_mls::{
    groups::{
        UpdateAdminListType,
        intents::{PermissionUpdateType, UpdateGroupMembershipResult},
        members::PermissionLevel,
    },
    subscriptions::SubscribeError,
};

#[derive(uniffi::Object, Clone)]
pub struct FfiConversation {
    pub(crate) inner: RustMlsGroup,
}

#[derive(uniffi::Object)]
pub struct FfiConversationListItem {
    pub(crate) conversation: FfiConversation,
    pub(crate) last_message: Option<FfiMessage>,
    pub(crate) is_commit_log_forked: Option<bool>,
}

#[uniffi::export]
impl FfiConversationListItem {
    pub fn conversation(&self) -> Arc<FfiConversation> {
        Arc::new(self.conversation.clone())
    }
    pub fn last_message(&self) -> Option<FfiMessage> {
        self.last_message.clone()
    }

    pub fn is_commit_log_forked(&self) -> Option<bool> {
        self.is_commit_log_forked
    }
}

#[derive(uniffi::Record, Debug)]
pub struct FfiUpdateGroupMembershipResult {
    added_members: HashMap<String, u64>,
    removed_members: Vec<String>,
    failed_installations: Vec<Vec<u8>>,
}

impl FfiUpdateGroupMembershipResult {
    fn new(
        added_members: HashMap<String, u64>,
        removed_members: Vec<String>,
        failed_installations: Vec<Vec<u8>>,
    ) -> Self {
        FfiUpdateGroupMembershipResult {
            added_members,
            removed_members,
            failed_installations,
        }
    }
}

impl From<UpdateGroupMembershipResult> for FfiUpdateGroupMembershipResult {
    fn from(value: UpdateGroupMembershipResult) -> Self {
        FfiUpdateGroupMembershipResult::new(
            value.added_members,
            value.removed_members,
            value.failed_installations,
        )
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiCreateGroupOptions {
    pub permissions: Option<FfiGroupPermissionsOptions>,
    pub group_name: Option<String>,
    pub group_image_url_square: Option<String>,
    pub group_description: Option<String>,
    pub custom_permission_policy_set: Option<FfiPermissionPolicySet>,
    pub message_disappearing_settings: Option<FfiMessageDisappearingSettings>,
    pub app_data: Option<String>,
}

impl FfiCreateGroupOptions {
    pub fn into_group_metadata_options(self) -> GroupMetadataOptions {
        GroupMetadataOptions {
            name: self.group_name,
            image_url_square: self.group_image_url_square,
            description: self.group_description,
            message_disappearing_settings: self
                .message_disappearing_settings
                .map(|settings| settings.into()),
            app_data: self.app_data,
        }
    }
}

#[derive(uniffi::Record, Clone, Default)]
pub struct FfiCreateDMOptions {
    pub message_disappearing_settings: Option<FfiMessageDisappearingSettings>,
}

impl FfiCreateDMOptions {
    pub fn new(disappearing_settings: FfiMessageDisappearingSettings) -> Self {
        FfiCreateDMOptions {
            message_disappearing_settings: Some(disappearing_settings),
        }
    }
    pub fn into_dm_metadata_options(self) -> DMMetadataOptions {
        DMMetadataOptions {
            message_disappearing_settings: self
                .message_disappearing_settings
                .map(|settings| settings.into()),
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiConversation {
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn send(
        &self,
        content_bytes: Vec<u8>,
        opts: FfiSendMessageOpts,
    ) -> Result<Vec<u8>, FfiError> {
        let message_id = self
            .inner
            .send_message(content_bytes.as_slice(), opts.into())
            .await?;
        Ok(message_id)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub(crate) async fn send_text(&self, text: &str) -> Result<Vec<u8>, FfiError> {
        let content =
            TextCodec::encode(text.to_string()).map_err(|e| FfiError::generic(e.to_string()))?;
        self.send(
            encoded_content_to_bytes(content),
            FfiSendMessageOpts {
                should_push: true,
                idempotency_key: None,
            },
        )
        .await
    }

    /// send a message without immediately publishing to the delivery service.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn send_optimistic(
        &self,
        content_bytes: Vec<u8>,
        opts: FfiSendMessageOpts,
    ) -> Result<Vec<u8>, FfiError> {
        let id = self
            .inner
            .send_message_optimistic(content_bytes.as_slice(), opts.into())?;

        Ok(id)
    }

    /// Delete a message by its ID. Returns the ID of the deletion message.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn delete_message(&self, message_id: Vec<u8>) -> Result<Vec<u8>, FfiError> {
        let deletion_id = self.inner.delete_message(message_id)?;
        Ok(deletion_id)
    }

    /// Publish all unpublished messages
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn publish_messages(&self) -> Result<(), FfiError> {
        self.inner.publish_messages().await?;
        Ok(())
    }

    /// Prepare a message for later publishing.
    /// Stores the message locally without publishing. Returns the message ID.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn prepare_message(
        &self,
        content_bytes: Vec<u8>,
        should_push: bool,
        idempotency_key: Option<String>,
    ) -> Result<Vec<u8>, FfiError> {
        let id = self.inner.prepare_message_for_later_publish(
            content_bytes.as_slice(),
            should_push,
            idempotency_key,
        )?;
        Ok(id)
    }

    /// Publish a previously prepared message by ID.
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn publish_stored_message(&self, message_id: Vec<u8>) -> Result<(), FfiError> {
        self.inner.publish_stored_message(&message_id).await?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn sync(&self) -> Result<(), FfiError> {
        self.inner.sync().await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn find_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessage>, FfiError> {
        let messages: Vec<FfiMessage> = self
            .inner
            .find_messages(&opts.into())?
            .into_iter()
            .map(|msg| msg.into())
            .collect();

        Ok(messages)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn count_messages(&self, opts: FfiListMessagesOptions) -> Result<i64, FfiError> {
        let count = self.inner.count_messages(&opts.into())?;

        Ok(count)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn find_messages_with_reactions(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<FfiMessageWithReactions>, FfiError> {
        let messages: Vec<FfiMessageWithReactions> = self
            .inner
            .find_messages_with_reactions(&opts.into())?
            .into_iter()
            .map(|msg| msg.into())
            .collect();
        Ok(messages)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn find_enriched_messages(
        &self,
        opts: FfiListMessagesOptions,
    ) -> Result<Vec<Arc<FfiDecodedMessage>>, FfiError> {
        let messages: Vec<Arc<FfiDecodedMessage>> = self
            .inner
            .find_messages_v2(&opts.into())?
            .into_iter()
            .map(|msg| Arc::new(msg.into()))
            .collect();
        Ok(messages)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn process_streamed_conversation_message(
        &self,
        envelope_bytes: Vec<u8>,
    ) -> Result<Vec<FfiMessage>, FfiError> {
        let message = self
            .inner
            .process_streamed_group_message(envelope_bytes)
            .await?;
        Ok(message.into_iter().map(Into::into).collect())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn list_members(&self) -> Result<Vec<FfiConversationMember>, FfiError> {
        let members: Vec<FfiConversationMember> = self
            .inner
            .members()
            .await?
            .into_iter()
            .map(|member| FfiConversationMember {
                inbox_id: member.inbox_id,
                account_identifiers: member.account_identifiers.to_ffi(),
                installation_ids: member.installation_ids,
                permission_level: match member.permission_level {
                    PermissionLevel::Member => FfiPermissionLevel::Member,
                    PermissionLevel::Admin => FfiPermissionLevel::Admin,
                    PermissionLevel::SuperAdmin => FfiPermissionLevel::SuperAdmin,
                },
                consent_state: member.consent_state.into(),
            })
            .collect();

        Ok(members)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn membership_state(&self) -> Result<FfiGroupMembershipState, FfiError> {
        let state = self.inner.membership_state()?;
        Ok(state.into())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn add_members_by_identity(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<FfiUpdateGroupMembershipResult, FfiError> {
        let account_identifiers = account_identifiers.to_internal()?;
        log::info!(
            "adding members: {}",
            account_identifiers
                .iter()
                .map(|ident| format!("{ident}"))
                .collect::<Vec<_>>()
                .join(",")
        );

        self.inner
            .add_members_by_identity(&account_identifiers)
            .await
            .map(FfiUpdateGroupMembershipResult::from)
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn add_members(
        &self,
        inbox_ids: Vec<String>,
    ) -> Result<FfiUpdateGroupMembershipResult, FfiError> {
        log::info!("Adding members by inbox ID: {}", inbox_ids.join(", "));

        self.inner
            .add_members(&inbox_ids)
            .await
            .map(FfiUpdateGroupMembershipResult::from)
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn remove_members_by_identity(
        &self,
        account_identifiers: Vec<FfiIdentifier>,
    ) -> Result<(), FfiError> {
        self.inner
            .remove_members_by_identity(&account_identifiers.to_internal()?)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn remove_members(&self, inbox_ids: Vec<String>) -> Result<(), FfiError> {
        let ids = inbox_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
        self.inner.remove_members(ids.as_slice()).await?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn leave_group(&self) -> Result<(), FfiError> {
        self.inner.leave_group().await?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn update_group_name(&self, group_name: String) -> Result<(), FfiError> {
        self.inner.update_group_name(group_name).await?;
        Ok(())
    }

    /// Enable AppData-proposal-based metadata updates on this group.
    ///
    /// Builds and stages the bootstrap commit that migrates this
    /// group's per-field metadata, admin lists, permissions, and
    /// membership from the legacy `GroupContextExtensions` shape into
    /// the unified OpenMLS `AppDataDictionary`. After it returns
    /// successfully, all subsequent metadata updates flow as
    /// `AppDataUpdate` proposals rather than GCE proposals.
    ///
    /// **Requires**: every existing member's latest key package must
    /// advertise `ProposalType::AppDataUpdate`. Hosts should ramp
    /// adoption with the migration code shipped before flipping any
    /// group; the call hard-fails with `ProposalsNotSupported` if
    /// any member lags. (The error currently surfaces a static
    /// message; structured per-inbox lag info is a future
    /// enhancement.)
    ///
    /// **One-way**: a migrated group cannot return to the legacy
    /// path. Operationally treated as a flag day per group.
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn enable_proposals(
        &self,
        options: FfiEnableProposalsOptions,
    ) -> Result<(), FfiError> {
        self.inner
            .enable_proposals(options.into())
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn group_name(&self) -> Result<String, FfiError> {
        let group_name = self.inner.group_name()?;
        Ok(group_name)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn update_app_data(&self, options: FfiUpdateAppDataOptions) -> Result<(), FfiError> {
        self.inner
            .update_app_data(options.value, options.expected_value)
            .await?;
        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn app_data(&self) -> Result<String, FfiError> {
        let app_data = self.inner.app_data()?;
        Ok(app_data)
    }

    /// Whether this group has migrated to AppData-proposal-based
    /// metadata updates (the `AppDataDictionary` group-context
    /// extension is present). `false` means the group is still on
    /// the legacy GroupContextExtensions path.
    ///
    /// Prefer this semantic bool over scanning
    /// [`FfiGroupMembershipCapabilities::context_extensions`] for
    /// `AppDataDictionary` — the capabilities snapshot answers
    /// "which members block migration", not "is this group migrated",
    /// and the marker extension is an internal protocol detail.
    /// Mirrors `proposalsEnabled` on the wasm and node bindings.
    #[tracing::instrument(level = "debug", skip_all)]
    pub fn proposals_enabled(&self) -> Result<bool, FfiError> {
        Ok(self.inner.is_proposals_enabled()?)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn update_group_image_url_square(
        &self,
        group_image_url_square: String,
    ) -> Result<(), FfiError> {
        self.inner
            .update_group_image_url_square(group_image_url_square)
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn group_image_url_square(&self) -> Result<String, FfiError> {
        Ok(self.inner.group_image_url_square()?)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn update_group_description(
        &self,
        group_description: String,
    ) -> Result<(), FfiError> {
        self.inner
            .update_group_description(group_description)
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn group_description(&self) -> Result<String, FfiError> {
        Ok(self.inner.group_description()?)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn update_conversation_message_disappearing_settings(
        &self,
        settings: FfiMessageDisappearingSettings,
    ) -> Result<(), FfiError> {
        self.inner
            .update_conversation_message_disappearing_settings(MessageDisappearingSettings::from(
                settings,
            ))
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn remove_conversation_message_disappearing_settings(&self) -> Result<(), FfiError> {
        self.inner
            .remove_conversation_message_disappearing_settings()
            .await?;

        Ok(())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn conversation_message_disappearing_settings(
        &self,
    ) -> Result<Option<FfiMessageDisappearingSettings>, FfiError> {
        let settings = self.inner.disappearing_settings()?;

        match settings {
            Some(s) => Ok(Some(FfiMessageDisappearingSettings::from(s))),
            None => Ok(None),
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn is_conversation_message_disappearing_enabled(&self) -> Result<bool, FfiError> {
        self.conversation_message_disappearing_settings()
            .map(|settings| {
                settings
                    .as_ref()
                    .is_some_and(|s| s.from_ns > 0 && s.in_ns > 0)
            })
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn admin_list(&self) -> Result<Vec<String>, FfiError> {
        self.inner.admin_list().map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn super_admin_list(&self) -> Result<Vec<String>, FfiError> {
        self.inner.super_admin_list().map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn is_admin(&self, inbox_id: &String) -> Result<bool, FfiError> {
        let admin_list = self.admin_list()?;
        Ok(admin_list.contains(inbox_id))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn is_super_admin(&self, inbox_id: &String) -> Result<bool, FfiError> {
        let super_admin_list = self.super_admin_list()?;
        Ok(super_admin_list.contains(inbox_id))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn add_admin(&self, inbox_id: String) -> Result<(), FfiError> {
        self.inner
            .update_admin_list(UpdateAdminListType::Add, inbox_id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn remove_admin(&self, inbox_id: String) -> Result<(), FfiError> {
        self.inner
            .update_admin_list(UpdateAdminListType::Remove, inbox_id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn add_super_admin(&self, inbox_id: String) -> Result<(), FfiError> {
        self.inner
            .update_admin_list(UpdateAdminListType::AddSuper, inbox_id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn remove_super_admin(&self, inbox_id: String) -> Result<(), FfiError> {
        self.inner
            .update_admin_list(UpdateAdminListType::RemoveSuper, inbox_id)
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn group_permissions(&self) -> Result<Arc<FfiGroupPermissions>, FfiError> {
        let permissions = self.inner.permissions()?;
        Ok(Arc::new(FfiGroupPermissions {
            inner: Arc::new(permissions),
        }))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn update_permission_policy(
        &self,
        permission_update_type: FfiPermissionUpdateType,
        permission_policy_option: FfiPermissionPolicy,
        metadata_field: Option<FfiMetadataField>,
    ) -> Result<(), FfiError> {
        self.inner
            .update_permission_policy(
                PermissionUpdateType::from(&permission_update_type),
                permission_policy_option.try_into()?,
                metadata_field.map(|field| MetadataField::from(&field)),
            )
            .await
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn stream(&self, message_callback: Arc<dyn FfiMessageCallback>) -> FfiStreamCloser {
        local_delivery::stream_messages(
            self.inner.context.clone(),
            DeliveryScope::Groups(vec![self.inner.group_id]),
            LocalDeliveryFilter::default(),
            message_callback,
        )
    }

    /// Read this conversation through bidi receipt. `from` opens independent replay.
    pub async fn message_reader(
        &self,
        from: Option<FfiDeliveryCursor>,
    ) -> Result<Arc<FfiMessageReader>, FfiError> {
        FfiMessageReader::open(
            self.inner.context.clone(),
            DeliveryScope::Groups(vec![self.inner.group_id]),
            LocalDeliveryFilter::default(),
            from,
        )
    }

    /// Read this conversation's retained messages and replay boundary from one snapshot.
    pub fn message_history_snapshot(
        &self,
        limit: u32,
    ) -> Result<FfiMessageHistorySnapshot, FfiError> {
        local_delivery::history_snapshot(
            &self.inner.context,
            DeliveryScope::Groups(vec![self.inner.group_id]),
            LocalDeliveryFilter::default(),
            limit,
        )
    }

    /// A database-bound replay cursor before the first retained delivery.
    pub fn beginning_delivery_cursor(&self) -> Result<FfiDeliveryCursor, FfiError> {
        local_delivery::beginning_cursor(&self.inner.context)
    }

    pub fn created_at_ns(&self) -> i64 {
        self.inner.created_at_ns
    }

    #[xmtp_common::err_span]
    pub fn is_active(&self) -> Result<bool, FfiError> {
        self.inner.is_active().map_err(Into::into)
    }

    #[xmtp_common::err_span]
    pub fn paused_for_version(&self) -> Result<Option<String>, FfiError> {
        self.inner.paused_for_version().map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn consent_state(&self) -> Result<FfiConsentState, FfiError> {
        self.inner
            .consent_state()
            .map(Into::into)
            .map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn update_consent_state(&self, state: FfiConsentState) -> Result<(), FfiError> {
        self.inner
            .update_consent_state(state.into())
            .map_err(Into::into)
    }

    #[xmtp_common::err_span]
    pub fn added_by_inbox_id(&self) -> Result<String, FfiError> {
        self.inner.added_by_inbox_id().map_err(Into::into)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn group_metadata(&self) -> Result<Arc<FfiConversationMetadata>, FfiError> {
        let metadata = self.inner.metadata().await?;
        Ok(Arc::new(FfiConversationMetadata {
            inner: Arc::new(metadata),
        }))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn dm_peer_inbox_id(&self) -> Option<String> {
        self.inner
            .dm_id
            .as_ref()
            .map(|dm_id| dm_id.other_inbox_id(self.inner.context.inbox_id()))
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn get_hmac_keys(&self) -> Result<HashMap<Vec<u8>, Vec<FfiHmacKey>>, FfiError> {
        let duplicate_dms = self.inner.find_duplicate_dms()?;

        let mut hmac_map = HashMap::new();
        for conversation in duplicate_dms {
            let id = conversation.group_id.to_vec();
            let keys = conversation
                .hmac_keys(-1..=1)?
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>();

            hmac_map.insert(id, keys);
        }

        let keys = self
            .inner
            .hmac_keys(-1..=1)?
            .into_iter()
            .map(Into::into)
            .collect::<Vec<_>>();

        hmac_map.insert(self.id(), keys);

        Ok(hmac_map)
    }

    #[xmtp_common::err_span]
    pub async fn conversation_debug_info(&self) -> Result<FfiConversationDebugInfo, FfiError> {
        let debug_info = self.inner.debug_info().await?;
        Ok(debug_info.into())
    }

    /// Snapshot this group's membership capabilities: the group context's
    /// extension types plus, per member inbox and installation, the extension
    /// types each advertises. Generic facts the caller filters — e.g. to
    /// answer whether the group is migrated to the proposal flow and which
    /// members block it. See
    /// [`xmtp_mls::groups::MlsGroup::membership_capabilities`].
    pub async fn membership_capabilities(
        &self,
    ) -> Result<FfiGroupMembershipCapabilities, FfiError> {
        let capabilities = self.inner.membership_capabilities().await?;
        Ok(capabilities.into())
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn find_duplicate_dms(&self) -> Result<Vec<Arc<FfiConversation>>, FfiError> {
        let dms = self.inner.find_duplicate_dms()?;

        let ffi_conversations: Vec<Arc<FfiConversation>> =
            dms.into_iter().map(|dm| Arc::new(dm.into())).collect();

        Ok(ffi_conversations)
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn get_last_read_times(&self) -> Result<HashMap<String, i64>, FfiError> {
        let latest_read_times = self.inner.get_last_read_times()?;
        Ok(latest_read_times)
    }
}

#[uniffi::export]
impl FfiConversation {
    pub fn id(&self) -> Vec<u8> {
        self.inner.group_id.to_vec()
    }

    pub fn conversation_type(&self) -> FfiConversationType {
        self.inner.conversation_type.into()
    }
}

type FfiHandle = Box<GenericStreamHandle<Result<(), SubscribeError>>>;

#[derive(uniffi::Object, Clone)]
pub struct FfiStreamCloser {
    stream_handle: Arc<Mutex<Option<FfiHandle>>>,
    // for convenience, does not require locking mutex.
    abort_handle: Arc<Box<dyn AbortHandle>>,
    pub(crate) message_control: Option<MessageReaderControl>,
}

impl FfiStreamCloser {
    pub fn new(
        stream_handle: impl StreamHandle<StreamOutput = Result<(), SubscribeError>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            abort_handle: Arc::new(stream_handle.abort_handle()),
            stream_handle: Arc::new(Mutex::new(Some(Box::new(stream_handle)))),
            message_control: None,
        }
    }
}

impl Drop for FfiStreamCloser {
    fn drop(&mut self) {
        if self.message_control.is_some() && Arc::strong_count(&self.stream_handle) == 1 {
            self.end();
        }
    }
}

#[uniffi::export(async_runtime = "tokio")]
impl FfiStreamCloser {
    /// Message streams have a snapshot. Other live-notification streams return None.
    pub fn catch_up_snapshot(&self) -> Option<FfiMessageCatchUpSnapshot> {
        self.message_control
            .as_ref()
            .map(|control| control.catch_up_snapshot().into())
    }

    /// Wait for message-stream status or close. Notification streams return None.
    pub async fn catch_up_changed(&self) -> Option<FfiMessageCatchUpSnapshot> {
        let control = self.message_control.as_ref()?;
        control.changed().await;
        Some(control.catch_up_snapshot().into())
    }

    /// Replace message scope and invalidate stale queued items. None selects all conversations.
    pub fn update_scope(&self, group_ids: Option<Vec<Vec<u8>>>) -> Result<(), FfiError> {
        let control = self
            .message_control
            .as_ref()
            .ok_or_else(|| FfiError::generic("This is not a message stream"))?;
        control.update_scope(local_delivery::delivery_scope(group_ids)?);
        Ok(())
    }

    /// Replace message filters without acknowledging queued items.
    pub fn update_filter(
        &self,
        conversation_type: Option<FfiConversationType>,
        consent_states: Option<Vec<FfiConsentState>>,
    ) -> Result<(), FfiError> {
        let control = self
            .message_control
            .as_ref()
            .ok_or_else(|| FfiError::generic("This is not a message stream"))?;
        control.update_filter(local_delivery::delivery_filter(
            conversation_type,
            consent_states,
        ));
        Ok(())
    }

    /// Fence message delivery immediately, then request stream shutdown without waiting.
    /// Pending messages remain unacknowledged.
    #[tracing::instrument(
        level = "debug",
        skip_all,
        fields(
            operation = "stream.end_stream",
            sentry.op = "stream",
            sentry.name = "stream.end_stream"
        )
    )]
    pub fn end(&self) {
        if let Some(control) = &self.message_control {
            control.close();
        }
        self.abort_handle.end();
    }

    /// End the stream and asynchronously wait for it to shutdown
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn end_and_wait(&self) -> Result<(), FfiError> {
        use xmtp_common::StreamHandleError::*;

        if let Some(control) = &self.message_control {
            control.close();
        }

        if self.abort_handle.is_finished() {
            return Ok(());
        }

        let mut stream_handle = self.stream_handle.lock().await;
        let stream_handle = stream_handle.take();
        if let Some(mut h) = stream_handle {
            match h.end_and_wait().await {
                Err(Cancelled) => Ok(()),
                Err(Panicked(msg)) => Err(FfiError::generic(msg)),
                Err(e) => Err(FfiError::generic(format!("error joining task {}", e))),
                Ok(t) => t.map_err(|e| FfiError::generic(e.to_string())),
            }
        } else {
            log::warn!("subscription already closed");
            Ok(())
        }
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub fn is_closed(&self) -> bool {
        self.abort_handle.is_finished()
    }

    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn wait_for_ready(&self) {
        let mut stream_handle = self.stream_handle.lock().await;
        if let Some(ref mut h) = *stream_handle {
            h.wait_for_ready().await;
        }
    }
}

/// SDK-owned callback boundary. Queue insertion does not acknowledge delivery.
#[uniffi::export(with_foreign)]
pub trait FfiMessageCallback: Send + Sync {
    /// Retain the token until app handoff. An error rejects the item and stops this stream.
    fn on_message(&self, delivery: FfiMessageDelivery) -> Result<(), FfiError>;
    fn on_error(&self, error: FfiError);
    fn on_close(&self);
}

#[uniffi::export(with_foreign)]
pub trait FfiConversationCallback: Send + Sync {
    fn on_conversation(&self, conversation: Arc<FfiConversation>);
    fn on_error(&self, error: FfiError);
    fn on_close(&self);
}

#[uniffi::export(with_foreign)]
pub trait FfiConsentCallback: Send + Sync {
    fn on_consent_update(&self, consent: Vec<FfiConsent>);
    fn on_error(&self, error: FfiError);
    fn on_close(&self);
}

#[uniffi::export(with_foreign)]
pub trait FfiPreferenceCallback: Send + Sync {
    fn on_preference_update(&self, preference: Vec<FfiPreferenceUpdate>);
    fn on_error(&self, error: FfiError);
    fn on_close(&self);
}

#[uniffi::export(with_foreign)]
pub trait FfiMessageDeletionCallback: Send + Sync {
    fn on_message_deleted(&self, message: Arc<FfiDecodedMessage>);
}
