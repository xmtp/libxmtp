//! XMTP Welcome Processing
//! Processes a new welcome from the network

use crate::groups::mls_ext::CommitLogStorer;
use crate::groups::mls_sync::DeferredEvents;
use crate::groups::{MetadataPermissionsError, mls_sync};
use crate::{
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, ValidateGroupMembership, mls_ext::DecryptedWelcome,
        validate_dm_group, validated_commit::LibXMTPVersion,
    },
    intents::ProcessIntentError,
    subscriptions::SyncWorkerEvent,
    track,
};
use derive_builder::Builder;
use openmls::group::MlsGroup as OpenMlsGroup;
use prost::Message;
use xmtp_common::RetryableError;
use xmtp_common::time::now_ns;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_db::{
    NotFound, StorageError, XmtpOpenMlsProvider, XmtpOpenMlsProviderRef,
    consent_record::{ConsentState, StoredConsentRecord},
    group::{ConversationType, GroupMembershipState, StoredGroup},
    group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    prelude::*,
    refresh_state::EntityKind,
};
use xmtp_mls_common::{
    group_metadata::extract_group_metadata, group_mutable_metadata::extract_group_mutable_metadata,
};
use xmtp_proto::xmtp::mls::{
    api::v1::welcome_message,
    message_contents::{ContentTypeId, GroupUpdated, group_updated::Inbox},
};

#[derive(Builder)]
#[builder(
    pattern = "owned",
    setter(strip_option),
    build_fn(error = "GroupError", private)
)]
pub struct XmtpWelcome<'a, C, V> {
    context: C,
    welcome: &'a welcome_message::V1,
    cursor_increment: bool,
    validator: V,
    /// Worker events collected throughout the welcome process
    #[builder(default = "Some(mls_sync::DeferredEvents::default())")]
    events: Option<mls_sync::DeferredEvents>,
}

impl<'a, C, V> XmtpWelcome<'a, C, V> {
    pub fn builder() -> XmtpWelcomeBuilder<'a, C, V> {
        Default::default()
    }
}

/// result of a commit
/// we consider a commit succesful if it either:
/// - Fails forever (can not be retried)
/// - returns a valid MLS Group
enum CommitResult<C> {
    /// Failed on a non-retryable error
    FailedForever(GroupError),
    /// Returns a valid MLS Group
    Ok(MlsGroup<C>),
}

impl<C> CommitResult<C> {
    fn into_result(self) -> Result<MlsGroup<C>, GroupError> {
        match self {
            Self::FailedForever(err) => Err(err),
            Self::Ok(group) => Ok(group),
        }
    }
}

impl<'a, C, V> XmtpWelcomeBuilder<'a, C, V>
where
    C: XmtpSharedContext,
    V: ValidateGroupMembership,
{
    pub async fn process(self) -> Result<MlsGroup<C>, GroupError> {
        let mut this = self.build()?;
        let db = this.context.db();
        if let Some(group) = this.check_if_processed(&db)? {
            return Ok(group);
        }

        match this.validate_membership(&db).await {
            Err(e) if !e.is_retryable() && this.cursor_increment => {
                tracing::info!(
                    "detected non-retryable error {e}, incrementing welcome cursor [{}]",
                    this.welcome.id
                );
                this.update_cursor(&db)?;
                return Err(e);
            }
            Err(e) => {
                return Err(e);
            }
            _ => (),
        }
        // we only use take once
        let mut events = this
            .events
            .take()
            .expect("builder is built with events as Some");
        let commit_result = this.commit_or_fail_forever(&mut events)?;
        commit_result.into_result()
    }
}

impl<'a, C, V> XmtpWelcome<'a, C, V>
where
    C: XmtpSharedContext,
    V: ValidateGroupMembership,
{
    /// Get the last cursor in the database for welcomes
    fn last_cursor(&self, db: &impl DbQuery) -> Result<i64, StorageError> {
        db.get_last_cursor_for_id(self.context.installation_id(), EntityKind::Welcome)
    }

    /// Update the cursor in the database
    /// returns true if the cursor was updated, otherwise false.
    fn update_cursor(&self, db: &impl DbQuery) -> Result<bool, StorageError> {
        db.update_cursor(
            self.context.installation_id(),
            EntityKind::Welcome,
            self.welcome.id as i64,
        )
    }

    /// Increment cursor only if the error is not retryable
    /// Check if the welcome has already been processed
    /// if the cursor of this welcome is less than the one we have in our local database,
    /// we can safely return the local cached group as if we had processed it.
    fn check_if_processed(&self, db: &impl DbQuery) -> Result<Option<MlsGroup<C>>, GroupError> {
        let context = &self.context;

        // Check if this welcome was already processed. Return the existing group if so.
        if self.last_cursor(db)? >= self.welcome.id as i64 {
            let group = db
                .find_group_by_welcome_id(self.welcome.id as i64)?
                // The welcome previously errored out, e.g. HPKE error, so it's not in the DB
                .ok_or(GroupError::NotFound(NotFound::GroupByWelcome(
                    self.welcome.id as i64,
                )))?;

            let group = MlsGroup::<_>::new(
                context.clone(),
                group.id,
                group.dm_id,
                group.conversation_type,
                group.created_at_ns,
            );

            tracing::warn!("Skipping old welcome {}", self.welcome.id);
            return Ok(Some(group));
        };
        Ok(None)
    }

    /// Process the welcome without affecting persistent state.
    /// Return error if validation fails or if welcome was already processed.
    async fn validate_membership(&self, db: &impl DbQuery) -> Result<(), GroupError> {
        let Self { welcome, .. } = self;
        let mut decrypt_result: Result<DecryptedWelcome, GroupError> =
            Err(GroupError::UninitializedResult);
        let transaction_result = self.context.mls_storage().transaction(|conn| {
            let mls_storage = conn.key_store();
            decrypt_result = DecryptedWelcome::from_encrypted_bytes(
                &XmtpOpenMlsProvider::new(mls_storage),
                &welcome.hpke_public_key,
                &welcome.data,
                welcome.wrapper_algorithm.into(),
            );
            Err(StorageError::IntentionalRollback)
        });

        let Err(StorageError::IntentionalRollback) = transaction_result else {
            return Err(transaction_result?);
        };

        let DecryptedWelcome { staged_welcome, .. } = decrypt_result?;
        // Ensure that the list of members in the group's MLS tree matches the list of inboxes specified
        // in the `GroupMembership` extension.
        self.validator
            .check_initial_membership(&staged_welcome)
            .await?;
        let group_id = staged_welcome.public_group().group_id();
        // try to load the group this welcome represents
        // defensive to avoid race conditions & duplicates
        if db.find_group(group_id.as_slice())?.is_some() {
            // Fetch the original MLS group, rather than the one from the welcome
            let result = MlsGroup::new_cached(self.context.clone(), group_id.as_slice());
            if result.is_err() {
                tracing::error!(
                    "Error fetching group while validating welcome: {:?}",
                    result.err()
                );
            } else {
                let (group, _) = result.expect("No error");
                // Check the group epoch as well, because we may not have synced the latest is_active state
                // TODO(rich): Design a better way to detect if incoming welcomes are valid
                if group.is_active()?
                    && staged_welcome
                        .public_group()
                        .group_context()
                        .epoch()
                        .as_u64()
                        <= group.epoch().await?
                {
                    tracing::error!(
                        "Skipping welcome {} because we are already in group {}",
                        welcome.id,
                        hex::encode(group_id.as_slice())
                    );
                    return Err(ProcessIntentError::WelcomeAlreadyProcessed(welcome.id).into());
                }
            }
        }
        Ok(())
    }

    /// Commit the welcome to the local db and memory.
    /// Verifies the welcome processed succesfully. If it fails on a non-retryable error,
    /// increments the cursor. Otherwise state must remain as if no transaction occurred.
    /// Returns an error if group failed to commit.
    /// Once transaction succeeds, sends device sync messages
    fn commit_or_fail_forever(
        &self,
        events: &mut DeferredEvents,
    ) -> Result<CommitResult<C>, GroupError> {
        let commit_result = self.context.mls_storage().transaction(|conn| {
            let storage = conn.key_store();
            // Savepoint transaction
            let result = storage.savepoint(|conn| self.commit(conn, events));
            let db = storage.db();
            // if we got an error
            // and the error is not retryable
            // and cursor increment is enabled
            // update the cursor
            match result {
                Err(err) if !err.is_retryable() && self.cursor_increment => {
                    tracing::warn!("welcome with cursor_id={} failed with a non-retryable error because of {err}, incrementing cursor", self.welcome.id);
                    self.update_cursor(&db)?;
                    // return ok to commit the transaction
                    Ok(CommitResult::FailedForever(err))
                },
                // roll everything back to retry
                Err(e) => Err(e),
                Ok(group) => Ok(CommitResult::Ok(group)),
            }
        })?;
        events.send_all(&self.context);
        Ok(commit_result)
    }

    /// The welcome was validated and we haven't processed yet.
    /// Can be commited
    /// Requires a transaction
    fn commit(
        &self,
        tx: &mut impl TransactionalKeyStore,
        events: &mut DeferredEvents,
    ) -> Result<MlsGroup<C>, GroupError> {
        let Self {
            welcome,
            cursor_increment,
            context,
            ..
        } = self;

        let storage = tx.key_store();
        let db = storage.db();
        let provider = XmtpOpenMlsProviderRef::new(&storage);
        let decrypted_welcome = DecryptedWelcome::from_encrypted_bytes(
            &provider,
            &welcome.hpke_public_key,
            &welcome.data,
            welcome.wrapper_algorithm.into(),
        )?;
        let DecryptedWelcome {
            staged_welcome,
            added_by_inbox_id,
            added_by_installation_id,
        } = decrypted_welcome;

        tracing::debug!("calling update cursor for welcome {}", welcome.id);
        let requires_processing = {
            let current_cursor = self.last_cursor(&db)?;
            welcome.id > current_cursor as u64
        };
        if !requires_processing {
            tracing::error!("Skipping already processed welcome {}", welcome.id);
            return Err(ProcessIntentError::WelcomeAlreadyProcessed(welcome.id).into());
        }
        if *cursor_increment {
            // TODO: We update the cursor if this welcome decrypts successfully, but if previous welcomes
            // failed due to retriable errors, this will permanently skip them.
            db.update_cursor(
                context.installation_id(),
                EntityKind::Welcome,
                welcome.id as i64,
            )?;
        }

        let mls_group = OpenMlsGroup::from_welcome_logged(
            &provider,
            staged_welcome,
            &added_by_inbox_id,
            &added_by_installation_id,
        )?;
        let group_id = mls_group.group_id().to_vec();
        let metadata =
            extract_group_metadata(&mls_group).map_err(MetadataPermissionsError::from)?;
        let dm_members = metadata.dm_members;
        let conversation_type = metadata.conversation_type;
        let mutable_metadata = extract_group_mutable_metadata(&mls_group).ok();
        let disappearing_settings = mutable_metadata.as_ref().and_then(|metadata| {
            MlsGroup::<C>::conversation_message_disappearing_settings_from_extensions(metadata).ok()
        });

        let paused_for_version: Option<String> = mutable_metadata.as_ref().and_then(|metadata| {
            let min_version = MlsGroup::<C>::min_protocol_version_from_extensions(metadata);
            if let Some(min_version) = min_version {
                let current_version_str = context.version_info().pkg_version();
                let current_version =
                    LibXMTPVersion::parse(current_version_str).ok()?;
                let required_min_version = LibXMTPVersion::parse(&min_version.clone()).ok()?;
                if required_min_version > current_version {
                    tracing::warn!(
                        "Saving group from welcome as paused since version requirements are not met. \
                        Group ID: {}, \
                        Required version: {}, \
                        Current version: {}",
                        hex::encode(group_id.clone()),
                        min_version,
                        current_version_str
                    );
                    Some(min_version)
                } else {
                    None
                }
            } else {
                None
            }
        });

        let mut group = StoredGroup::builder();
        group
            .id(group_id)
            .created_at_ns(now_ns())
            .added_by_inbox_id(&added_by_inbox_id)
            .welcome_id(welcome.id as i64)
            .conversation_type(conversation_type)
            .dm_id(dm_members.map(String::from))
            .message_disappear_from_ns(disappearing_settings.as_ref().map(|m| m.from_ns))
            .message_disappear_in_ns(disappearing_settings.as_ref().map(|m| m.in_ns))
            .should_publish_commit_log(MlsGroup::<C>::check_should_publish_commit_log(
                context.inbox_id().to_string(),
                mutable_metadata,
            ));

        let to_store = match conversation_type {
            ConversationType::Group => group
                .membership_state(GroupMembershipState::Pending)
                .paused_for_version(paused_for_version)
                .build()?,
            ConversationType::Dm => {
                validate_dm_group(context, &mls_group, &added_by_inbox_id)?;
                group
                    .membership_state(GroupMembershipState::Pending)
                    .last_message_ns(welcome.created_ns as i64)
                    .build()?
            }
            ConversationType::Sync => {
                // Let the DeviceSync worker know about the presence of a new
                // sync group that came in from a welcome.3
                let group_id = mls_group.group_id().to_vec();
                events.add_worker_event(SyncWorkerEvent::NewSyncGroupFromWelcome(group_id));

                group
                    .membership_state(GroupMembershipState::Allowed)
                    .build()?
            }
        };

        tracing::warn!("storing group with welcome id {}", welcome.id);
        // Insert or replace the group in the database.
        // Replacement can happen in the case that the user has been removed from and subsequently re-added to the group.
        let stored_group = db.insert_or_replace_group(to_store)?;

        StoredConsentRecord::stitch_dm_consent(&db, &stored_group)?;
        track!(
            "Group Welcome",
            {
                "conversation_type": stored_group.conversation_type,
                "added_by_inbox_id": &stored_group.added_by_inbox_id
            },
            group: &stored_group.id
        );

        // Create a GroupUpdated payload
        let current_inbox_id = context.inbox_id().to_string();
        let added_payload = GroupUpdated {
            initiated_by_inbox_id: added_by_inbox_id.clone(),
            added_inboxes: vec![Inbox {
                inbox_id: current_inbox_id.clone(),
            }],
            removed_inboxes: vec![],
            metadata_field_changes: vec![],
        };

        let encoded_added_payload = GroupUpdatedCodec::encode(added_payload)?;
        let mut encoded_added_payload_bytes = Vec::new();
        encoded_added_payload
            .encode(&mut encoded_added_payload_bytes)
            .map_err(GroupError::EncodeError)?;

        let added_message_id = crate::utils::id::calculate_message_id(
            &stored_group.id,
            encoded_added_payload_bytes.as_slice(),
            &format!("{}_welcome_added", welcome.created_ns),
        );

        let added_content_type = match encoded_added_payload.r#type {
            Some(ct) => ct,
            None => {
                tracing::warn!(
                    "Missing content type in encoded added payload, using default values"
                );
                ContentTypeId {
                    authority_id: "unknown".to_string(),
                    type_id: "unknown".to_string(),
                    version_major: 0,
                    version_minor: 0,
                }
            }
        };

        let added_msg = StoredGroupMessage {
            id: added_message_id,
            group_id: stored_group.id.clone(),
            decrypted_message_bytes: encoded_added_payload_bytes,
            sent_at_ns: welcome.created_ns as i64,
            kind: GroupMessageKind::MembershipChange,
            sender_installation_id: welcome.installation_key.clone(),
            sender_inbox_id: added_by_inbox_id,
            delivery_status: DeliveryStatus::Published,
            content_type: added_content_type.type_id.into(),
            version_major: added_content_type.version_major as i32,
            version_minor: added_content_type.version_minor as i32,
            authority_id: added_content_type.authority_id,
            reference_id: None,
            sequence_id: Some(welcome.id as i64),
            originator_id: None,
            expire_at_ns: None,
        };

        added_msg.store_or_ignore(&db)?;

        tracing::info!(
            "[{}]: Created GroupUpdated message for welcome",
            current_inbox_id
        );

        let group = MlsGroup::new(
            context.clone(),
            stored_group.id,
            stored_group.dm_id,
            stored_group.conversation_type,
            stored_group.created_at_ns,
        );

        // If this group is created by us - auto-consent to it.
        if context.inbox_id() == metadata.creator_inbox_id {
            group.quietly_update_consent_state(ConsentState::Allowed, &db)?;
        }

        Ok(group)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        groups::test::NoopValidator,
        test::mock::{NewMockContext, context},
    };

    use super::*;

    fn generate_welcome() -> welcome_message::V1 {
        welcome_message::V1 {
            id: 0,
            created_ns: 0,
            installation_key: vec![0],
            data: vec![0],
            hpke_public_key: vec![],
            wrapper_algorithm: 0,
            welcome_metadata: vec![0],
        }
    }

    // Is async so that the async timeout from rstest is used in wasm (does not spawn thread)
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn welcome_builds_with_default_events(context: NewMockContext) {
        let w = generate_welcome();
        let builder = XmtpWelcome::builder()
            .context(context)
            .welcome(&w)
            .cursor_increment(true)
            .validator(NoopValidator)
            .build();
        assert!(builder.unwrap().events.is_some());
    }
}
