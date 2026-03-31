//! XMTP Welcome Processing
//! Processes a new welcome from the network

use std::collections::HashSet;

use crate::groups::mls_ext::CommitLogStorer;
use crate::groups::mls_sync::DeferredEvents;
use crate::groups::oneshot::Oneshot;
use crate::groups::{MetadataPermissionsError, mls_sync};
use crate::{
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, ValidateGroupMembership, mls_ext::DecryptedWelcome,
        validate_dm_group, validated_commit::LibXMTPVersion,
    },
    intents::ProcessIntentError,
    subscriptions::SyncWorkerEvent,
};
use derive_builder::Builder;
use openmls::group::MlsGroup as OpenMlsGroup;
use prost::Message;
use xmtp_common::RetryableError;
use xmtp_common::time::now_ns;
use xmtp_configuration::Originators;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_db::{
    StorageError, XmtpOpenMlsProviderRef,
    consent_record::{ConsentState, StoredConsentRecord},
    group::{ConversationType, GroupMembershipState, StoredGroup},
    group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    prelude::*,
    refresh_state::EntityKind,
};
use xmtp_mls_common::{
    group_metadata::extract_group_metadata, group_mutable_metadata::extract_group_mutable_metadata,
};
use xmtp_proto::types::Cursor;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, GroupUpdated, group_updated::Inbox};

/// Create a group from a decrypted and decoded welcome message.
/// If the group already exists in the store, overwrite the MLS state and do not update the group entry
///
/// # Parameters
/// * `context` - The client context to use for group operations
/// * `welcome` - The encrypted welcome message
/// * `cursor_increment` - Controls whether to allow cursor increments during processing.
///   Set to `true` when processing messages from trusted ordered sources (queries), and `false` when
///   processing from potentially out-of-order sources like streams.
/// * `validator` - The validator to use to check the group membership
#[derive(Builder)]
#[builder(
    pattern = "owned",
    setter(strip_option),
    build_fn(error = "GroupError", private)
)]
pub struct XmtpWelcome<'a, C, V> {
    context: C,
    welcome: &'a xmtp_proto::types::WelcomeMessage,
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
/// we consider a commit successful if it either:
/// - Fails forever (can not be retried)
/// - Was successfully decrypted and processed
enum CommitResult<C> {
    /// Failed on a non-retryable error
    FailedForever(GroupError),
    /// Successfully decrypted and processed
    Ok(Option<MlsGroup<C>>),
}

impl<C> CommitResult<C> {
    fn into_result(self) -> Result<Option<MlsGroup<C>>, GroupError> {
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
    #[tracing::instrument(skip_all, level = "trace")]
    pub async fn process(self) -> Result<Option<MlsGroup<C>>, GroupError> {
        let mut this = self.build()?;
        let db = this.context.db();
        if let Some(group) = this.check_if_processed(&db)? {
            return Ok(Some(group));
        }

        let decrypted_welcome = match this.validate_membership(&db).await {
            Err(e) if !e.is_retryable() && this.cursor_increment => {
                tracing::info!(
                    "detected non-retryable error {e}, incrementing welcome cursor [{}]",
                    this.welcome.cursor
                );
                this.update_cursor(&db)?;
                return Err(e);
            }
            Err(e) => {
                return Err(e);
            }
            Ok(decrypted_welcome) => decrypted_welcome,
        };
        // we only use take once
        let mut events = this
            .events
            .take()
            .expect("builder is built with events as Some");
        let commit_result = this.commit_or_fail_forever(decrypted_welcome, &mut events)?;
        commit_result.into_result()
    }
}

impl<'a, C, V> XmtpWelcome<'a, C, V>
where
    C: XmtpSharedContext,
    V: ValidateGroupMembership,
    <C::MlsStorage as XmtpMlsStorageProvider>::Connection: xmtp_db::ConnectionExt,
{
    /// Get the last cursor in the database for welcomes
    fn last_sequence_id(&self, db: &impl DbQuery) -> Result<i64, StorageError> {
        let last = db.get_last_cursor_for_originator(
            self.context.installation_id(),
            EntityKind::Welcome,
            self.welcome.originator_id(),
        )?;
        Ok(last.sequence_id as i64)
    }

    /// Update the cursor in the database
    /// returns true if the cursor was updated, otherwise false.
    fn update_cursor(&self, db: &impl DbQuery) -> Result<bool, StorageError> {
        db.update_cursor(
            self.context.installation_id(),
            EntityKind::Welcome,
            self.welcome.cursor,
        )
    }

    /// Increment cursor only if the error is not retryable
    /// Check if the welcome has already been processed
    /// if the cursor of this welcome is less than the one we have in our local database,
    /// we can safely return the local cached group as if we had processed it.
    fn check_if_processed(&self, db: &impl DbQuery) -> Result<Option<MlsGroup<C>>, GroupError> {
        if self.welcome.resuming() {
            return Ok(None);
        }
        let context = &self.context;

        // Check if this welcome was already processed. Return the existing group if so.
        if self.last_sequence_id(db)? >= self.welcome.sequence_id() as i64 {
            tracing::debug!(
                welcome_id = %self.welcome.cursor,
                "Welcome id is less than cursor, fetching from DB"
            );
            let maybe_group = db.find_group_by_sequence_id(self.welcome.cursor)?;
            let Some(group) = maybe_group else {
                tracing::warn!(
                    welcome_id = %self.welcome.cursor,
                    "Already processed welcome not found in DB, likely pre-existing group or oneshot message"
                );
                return Ok(None);
            };

            let group = MlsGroup::<_>::new(
                context.clone(),
                group.id,
                group.dm_id,
                group.conversation_type,
                group.created_at_ns,
            );

            tracing::warn!("Skipping old welcome {}", self.welcome.cursor);
            return Ok(Some(group));
        };
        Ok(None)
    }

    /// Process the welcome without affecting persistent state.
    /// Return error if validation fails or if welcome was already processed.
    async fn validate_membership(&self, db: &impl DbQuery) -> Result<DecryptedWelcome, GroupError> {
        let Self { welcome, .. } = self;
        let decrypted_welcome =
            DecryptedWelcome::from_welcome_proto(welcome, &self.context).await?;

        let DecryptedWelcome { staged_welcome, .. } = &decrypted_welcome;
        // Ensure that the list of members in the group's MLS tree matches the list of inboxes specified
        // in the `GroupMembership` extension.
        self.validator
            .check_initial_membership(staged_welcome)
            .await?;
        let group_id = staged_welcome.public_group().group_id();
        // try to load the group this welcome represents
        // defensive to avoid race conditions & duplicates
        if db.find_group(group_id.as_slice())?.is_some() {
            // Fetch the original MLS group, rather than the one from the welcome
            let result = MlsGroup::new_cached(self.context.clone(), group_id.as_slice());
            if let Ok((group, _)) = result {
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
                    tracing::warn!(
                        "Skipping welcome {} because we are already in group {}",
                        welcome.cursor,
                        hex::encode(group_id.as_slice())
                    );
                    return Err(ProcessIntentError::WelcomeAlreadyProcessed(welcome.cursor).into());
                }
            } else {
                tracing::error!(
                    "Error fetching group while validating welcome: {:?}",
                    result.err()
                );
            }
        }
        Ok(decrypted_welcome)
    }

    /// Commit the welcome to the local db and memory.
    /// Verifies the welcome processed successfully. If it fails on a non-retryable error,
    /// increments the cursor. Otherwise state must remain as if no transaction occurred.
    /// Returns an error if group failed to commit.
    /// Once transaction succeeds, sends device sync messages
    fn commit_or_fail_forever(
        &self,
        decrypted_welcome: DecryptedWelcome,
        events: &mut DeferredEvents,
    ) -> Result<CommitResult<C>, GroupError> {
        tracing::info!("attempting to commit welcome={}", &self.welcome.cursor);
        let commit_result = self.context.mls_storage().transaction(|conn| {
            let storage = conn.key_store();
            // Savepoint transaction
            let result = storage.savepoint(|conn| self.commit(conn, events, decrypted_welcome));
            let db = storage.db();
            // if we got an error
            // and the error is not retryable
            // and cursor increment is enabled
            // update the cursor
            match result {
                Err(err) if !err.is_retryable() && self.cursor_increment => {
                    tracing::warn!("welcome with cursor_id={} failed with a non-retryable error because of {err}, incrementing cursor", self.welcome.cursor);
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
    /// Can be committed
    /// Requires a transaction
    fn commit(
        &self,
        tx: &mut impl TransactionalKeyStore,
        events: &mut DeferredEvents,
        decrypted_welcome: DecryptedWelcome,
    ) -> Result<Option<MlsGroup<C>>, GroupError> {
        let Self {
            welcome,
            cursor_increment,
            context,
            ..
        } = self;

        let storage = tx.key_store();
        let db = storage.db();
        let provider = XmtpOpenMlsProviderRef::new(&storage);

        let DecryptedWelcome {
            staged_welcome,
            added_by_inbox_id,
            added_by_installation_id,
            welcome_metadata,
        } = decrypted_welcome;

        tracing::debug!("calling update cursor for welcome {}", welcome.cursor);
        let requires_processing =
            welcome.resuming() || welcome.sequence_id() > self.last_sequence_id(&db)? as u64;
        if !requires_processing {
            tracing::error!("Skipping already processed welcome {}", welcome.cursor);
            return Err(ProcessIntentError::WelcomeAlreadyProcessed(welcome.cursor).into());
        }
        if *cursor_increment {
            tracing::info!("updating cursor to {}", welcome.cursor);
            // TODO: We update the cursor if this welcome decrypts successfully, but if previous welcomes
            // failed due to retriable errors, this will permanently skip them.
            db.update_cursor(
                context.installation_id(),
                EntityKind::Welcome,
                welcome.cursor,
            )?;
        }
        let metadata =
            extract_group_metadata(staged_welcome.public_group().group_context().extensions())
                .map_err(MetadataPermissionsError::from)?;
        if metadata.conversation_type == ConversationType::Oneshot {
            Oneshot::process_welcome(
                &provider,
                welcome.cursor,
                added_by_inbox_id,
                added_by_installation_id,
                metadata,
            )?;
            return Ok(None);
        }

        // Extract group_id before consuming staged_welcome
        let group_id = staged_welcome.public_group().group_id().to_vec();
        let existing_group = db.find_group(group_id.as_slice())?;

        // Check if this is a re-add scenario:
        // - Self-removal (PendingRemove): user left voluntarily, then gets re-added
        // - Removal by others: user was removed by another member (membership state stays Allowed
        //   but MLS group is inactive)
        // We verify it's a NEW welcome by checking that:
        // 1. The existing group has a valid sequence_id (Some)
        // 2. The new welcome's sequence_id is GREATER than the existing one
        // This prevents incorrectly treating backup/restore or groups without sequence_ids as re-adds
        let is_readd_after_leaving = existing_group.as_ref().is_some_and(|g| {
            g.membership_state == GroupMembershipState::PendingRemove
                && matches!(g.sequence_id, Some(seq) if (welcome.cursor.sequence_id as i64) > seq)
        });

        let mls_group = OpenMlsGroup::from_welcome_logged(
            &provider,
            staged_welcome,
            &added_by_inbox_id,
            &added_by_installation_id,
        )?;
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

        // Determine the membership state
        // If the user is being re-added after leaving, set to ALLOWED
        // Otherwise, new members start in PENDING state
        let membership_state = if is_readd_after_leaving {
            tracing::info!(
                group_id = hex::encode(&group_id),
                "User is being re-added after leaving/removal, setting membership state to ALLOWED"
            );
            GroupMembershipState::Allowed
        } else {
            tracing::debug!(
                group_id = hex::encode(&group_id),
                "User is being added to new group, setting membership state to PENDING"
            );
            GroupMembershipState::Pending
        };

        let mut group = StoredGroup::builder();
        group
            .id(group_id)
            .created_at_ns(now_ns())
            .added_by_inbox_id(&added_by_inbox_id)
            .cursor(welcome.cursor)
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
                .membership_state(membership_state)
                .paused_for_version(paused_for_version)
                .build()?,
            ConversationType::Dm => {
                validate_dm_group(context, &mls_group, &added_by_inbox_id)?;
                group
                    .membership_state(membership_state)
                    .last_message_ns(welcome.timestamp())
                    .build()?
            }
            ConversationType::Sync => {
                // Let the DeviceSync worker know about the presence of a new
                // sync group that came in from a welcome.3
                let group_id = mls_group.group_id().to_vec();
                events.add_worker_event(SyncWorkerEvent::NewSyncGroupFromWelcome(group_id));

                // Sync groups are always Allowed.
                group
                    .membership_state(GroupMembershipState::Allowed)
                    .build()?
            }
            ConversationType::Oneshot => {
                unreachable!("StagedWelcome of type Oneshot should already be handled")
            }
        };

        tracing::info!("storing group with welcome id {}", welcome.cursor);

        // If this is a re-add after leaving, update the existing group's membership state
        // before calling insert_or_replace_group
        if is_readd_after_leaving && let Some(ref existing) = existing_group {
            tracing::info!(
                group_id = hex::encode(&existing.id),
                "Updating existing group membership state from PENDING_REMOVE to ALLOWED"
            );
            db.update_group_membership(&existing.id, GroupMembershipState::Allowed)?;
        }

        // Insert or replace the group in the database.
        // For existing groups, this only updates the sequence_id (not membership_state).
        let stored_group = db.insert_or_replace_group(to_store)?;

        StoredConsentRecord::stitch_dm_consent(&db, &stored_group)?;

        // Create a GroupUpdated payload
        let current_inbox_id = context.inbox_id().to_string();
        let added_payload = GroupUpdated {
            initiated_by_inbox_id: added_by_inbox_id.clone(),
            added_inboxes: vec![Inbox {
                inbox_id: current_inbox_id.clone(),
            }],
            removed_inboxes: vec![],
            metadata_field_changes: vec![],
            left_inboxes: vec![],
            added_admin_inboxes: vec![],
            removed_admin_inboxes: vec![],
            added_super_admin_inboxes: vec![],
            removed_super_admin_inboxes: vec![],
        };

        let encoded_added_payload = GroupUpdatedCodec::encode(added_payload)?;
        let mut encoded_added_payload_bytes = Vec::new();
        encoded_added_payload.encode(&mut encoded_added_payload_bytes)?;

        let added_message_id = crate::utils::id::calculate_message_id(
            &stored_group.id,
            encoded_added_payload_bytes.as_slice(),
            &format!("{}_welcome_added", welcome.created_ns),
        );

        let added_content_type = encoded_added_payload.r#type.unwrap_or_else(|| {
            tracing::warn!("Missing content type in encoded added payload, using default values");
            ContentTypeId {
                authority_id: "unknown".to_string(),
                type_id: "unknown".to_string(),
                version_major: 0,
                version_minor: 0,
            }
        });

        let cursor = welcome_metadata
            .map(|m| m.message_cursor as i64)
            .unwrap_or_default();

        // this is the commit that brought us into the group
        let added_msg = StoredGroupMessage {
            id: added_message_id,
            group_id: stored_group.id.clone(),
            decrypted_message_bytes: encoded_added_payload_bytes,
            sent_at_ns: welcome.timestamp(),
            kind: GroupMessageKind::MembershipChange,
            sender_installation_id: added_by_installation_id,
            sender_inbox_id: added_by_inbox_id,
            delivery_status: DeliveryStatus::Published,
            content_type: added_content_type.type_id.into(),
            version_major: added_content_type.version_major as i32,
            version_minor: added_content_type.version_minor as i32,
            authority_id: added_content_type.authority_id,
            reference_id: None,
            sequence_id: cursor,
            originator_id: Originators::MLS_COMMITS as i64,
            expire_at_ns: None,
            inserted_at_ns: 0, // Will be set by database
            should_push: true,
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
        } else if is_readd_after_leaving {
            // If user is being re-added after leaving, reset consent to Unknown
            // This requires the user to explicitly accept being added back
            tracing::info!(
                group_id = hex::encode(&group.group_id),
                "Resetting consent state to Unknown for re-added user"
            );
            group.quietly_update_consent_state(ConsentState::Unknown, &db)?;
        }

        db.update_cursor(
            &group.group_id,
            EntityKind::CommitMessage,
            //TODO:d14n this must change before D14n-only
            //Originator must be included in welcome
            Cursor::mls_commits(cursor as u64),
        )?;
        MlsGroup::<C>::mark_readd_requests_as_responded(
            &storage,
            &group.group_id,
            &HashSet::from([context.installation_id().to_vec()]),
            cursor,
        )?;

        tracing::info!(
            inbox_id = %current_inbox_id,
            installation_id = %self.context.installation_id(),
            group_id = %hex::encode(&group.group_id),
            welcome_id = welcome.cursor.sequence_id,
            originator_id = welcome.cursor.originator_id,
            cursor = cursor,
            "updated message cursor from welcome metadata"
        );

        Ok(Some(group))
    }
}

#[cfg(test)]
mod tests {
    use xmtp_common::Generate;

    use crate::{
        groups::test::NoopValidator,
        test::mock::{NewMockContext, context},
    };

    use super::*;

    // Is async so that the async timeout from rstest is used in wasm (does not spawn thread)
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn welcome_builds_with_default_events(context: NewMockContext) {
        let w = xmtp_proto::types::WelcomeMessage::generate();
        let builder = XmtpWelcome::builder()
            .context(context)
            .welcome(&w)
            .cursor_increment(true)
            .validator(NoopValidator)
            .build();
        assert!(builder.unwrap().events.is_some());
    }
}
