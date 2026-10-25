//! XMTP Welcome Processing
//! Processes a new welcome from the network

use std::collections::HashSet;

use crate::groups::MetadataPermissionsError;
use crate::groups::mls_ext::CommitLogStorer;
use crate::groups::mls_ext::ResolvedWelcome;
use crate::groups::oneshot::Oneshot;
use crate::groups::welcomes::WelcomeMembership;
use crate::identity_updates::{
    IdentityDependencyError, IdentityRequirement, InstallationDiffError,
};
use crate::state_tx::{state_write, state_write_with_events};
use crate::subscriptions::internal::{InternalEvent, PreferenceOrigin, emit_preference_updates};
use crate::worker::device_sync::is_own_sync_group;
use crate::{
    context::XmtpSharedContext,
    groups::{
        GroupError, MlsGroup, ValidateGroupMembership, mls_ext::DecryptedWelcome, validate_dm_group,
    },
    intents::ProcessIntentError,
};
use derive_builder::Builder;
use openmls::group::MlsGroup as OpenMlsGroup;
use prost::Message;
use xmtp_common::{snippet::Snippet, time::now_ns};
use xmtp_content_types::ContentCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_db::TransactionOutcome::{Continue, Rollback};
use xmtp_db::{
    TransactionOutcome, XmtpOpenMlsProviderRef,
    consent_record::{ConsentState, ConsentType, StoredConsentRecord},
    group::{ConversationType, GroupMembershipState, StoredGroup},
    group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    incoming_envelope::{JoinAnchorMode, NetworkEntityKind, StoredIncomingEnvelope, StreamTopic},
    prelude::*,
    refresh_state::EntityKind,
};
use xmtp_events::{
    ClientEvent, ConversationJoined, ConversationType as EventConversationType, EventBuffer,
    EventWriter, JoinOrigin,
};
use xmtp_mls_common::app_data::group_root::check_group_root;
use xmtp_mls_common::group_metadata::extract_group_metadata;
use xmtp_mls_common::libxmtp_version::LibXMTPVersion;

use xmtp_mls_common::app_data::component_source::extract_group_mutable_metadata_capability_aware;
use xmtp_proto::types::Cursor;
use xmtp_proto::xmtp::mls::message_contents::{ContentTypeId, GroupUpdated, group_updated::Inbox};

use xmtp_proto::types::GroupId;
/// Create a group from a decrypted and decoded welcome message.
/// An existing group can be replaced only after its removal has been processed.
///
/// # Parameters
/// * `context` - The client context to use for group operations
/// * `welcome` - The encrypted welcome message
/// * `pending` - The exact durable Welcome row that this attempt must complete.
/// * `validator` - The validator to use to check the group membership
#[derive(Builder)]
#[builder(
    pattern = "owned",
    setter(strip_option),
    build_fn(error = "GroupError", private)
)]
pub struct XmtpWelcome<'a, C, V> {
    context: C,
    /// Immutable network input. Each attempt reads its private keys again.
    welcome: &'a xmtp_proto::types::WelcomeMessage,
    /// Exact durable row to complete in the same transaction as the join.
    pending: StoredIncomingEnvelope,
    validator: V,
}

impl<'a, C, V> XmtpWelcome<'a, C, V> {
    pub fn builder() -> XmtpWelcomeBuilder<'a, C, V> {
        Default::default()
    }
}

/// A committed join or safe rejection that completes the pending Welcome row.
enum CommitResult<C> {
    /// Invalid input was rejected without keeping trial MLS writes.
    FailedForever(GroupError),
    /// Successfully decrypted and processed
    Ok(Option<MlsGroup<C>>),
}

/// Only invalid input can complete a rejected Welcome. Local failures remain pending.
pub(crate) fn terminal_welcome_error(error: &GroupError) -> bool {
    use openmls::prelude::WelcomeError;
    matches!(
        error,
        GroupError::InvalidWelcomeMetadata
            | GroupError::WelcomeGroupRoot(_)
            | GroupError::InvalidGroupMembership
            | GroupError::MetadataPermissionsError(_)
            | GroupError::NoPSKSupport
            | GroupError::TlsError(_)
            | GroupError::CredentialError(_)
            | GroupError::Identity(
                crate::identity::IdentityError::Decode(_)
                    | crate::identity::IdentityError::BasicCredential(_)
            )
            | GroupError::ConversionError(_)
            | GroupError::UnwrapWelcome(_)
            | GroupError::ProcessIntent(ProcessIntentError::WelcomeAlreadyProcessed(_))
            | GroupError::InstallationDiff(InstallationDiffError::IdentityDependency(
                IdentityDependencyError::InvalidSequence(_)
                    | IdentityDependencyError::MissingReference(_)
            ))
            | GroupError::WelcomeError(
                WelcomeError::GroupSecrets(_)
                    | WelcomeError::CiphersuiteMismatch
                    | WelcomeError::GroupInfo(_)
                    | WelcomeError::JoinerSecretNotFound
                    | WelcomeError::MissingRatchetTree
                    | WelcomeError::ConfirmationTagMismatch
                    | WelcomeError::InvalidGroupInfoSignature
                    | WelcomeError::UnknownSender
                    | WelcomeError::NotAWelcomeMessage
                    | WelcomeError::MalformedWelcomeMessage
                    | WelcomeError::UnableToDecrypt
                    | WelcomeError::PublicTreeError(_)
                    | WelcomeError::LeafNodeValidation(_)
            )
    )
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
    /// Resolve dependencies from a rolled-back trial, then install from fresh state.
    // Named explicitly (derived `mls.process` is too generic) and without `err`:
    // duplicate welcomes exit as Err(WelcomeAlreadyProcessed), an expected
    // outcome; unexpected failures set status on mls.process_new_welcome above.
    #[tracing::instrument(skip_all, fields(operation = "mls.process_welcome"))]
    pub async fn process(self) -> Result<Option<MlsGroup<C>>, GroupError> {
        let this = self.build()?;
        this.check_pending(&this.context.db())?;

        let (resolved, membership) = match this.validate_membership().await {
            Err(error) => return this.reject_or_retry(error),
            Ok(validated) => validated,
        };
        let commit_result = this.commit_or_fail_forever(&resolved, Some(&membership), None)?;
        commit_result.into_result()
    }

    /// Restage under the writer and return missing dependencies to the scheduler.
    /// Only a matching resolver result can prove an identity reference is absent.
    pub(crate) fn process_resolved(
        self,
        resolved: &ResolvedWelcome,
        missing_reference: Option<&IdentityRequirement>,
    ) -> Result<Option<MlsGroup<C>>, GroupError> {
        let this = self.build()?;
        this.commit_or_fail_forever(resolved, None, missing_reference)?
            .into_result()
    }
}

impl<'a, C, V> XmtpWelcome<'a, C, V>
where
    C: XmtpSharedContext,
    V: ValidateGroupMembership,
    <C::MlsStorage as XmtpMlsStorageProvider>::Connection: xmtp_db::ConnectionExt,
{
    fn topic(&self) -> StreamTopic {
        StreamTopic {
            entity_id: self.context.installation_id().to_vec(),
            kind: NetworkEntityKind::Welcome,
        }
    }

    /// Require the same pending row before any join or rejection can commit.
    fn check_pending(&self, db: &impl DbQuery) -> Result<(), GroupError> {
        let current = db.pending_envelope(&self.topic(), self.welcome.cursor)?;
        if current.is_none_or(|row| row.envelope != self.pending.envelope)
            || self.pending.sequence_id != self.welcome.cursor.0 as i64
            || self.pending.entity_id != self.context.installation_id()
        {
            return Err(ProcessIntentError::WelcomeAlreadyProcessed(self.welcome.cursor).into());
        }
        Ok(())
    }

    fn reject_or_retry(&self, error: GroupError) -> Result<Option<MlsGroup<C>>, GroupError> {
        if terminal_welcome_error(&error) {
            state_write(self.context.mls_storage(), |tx| {
                let storage = tx.storage();
                let db = storage.db();
                self.check_pending(&db)?;
                db.record_terminal_rejection(
                    &self.topic(),
                    self.welcome.cursor,
                    "invalid_welcome",
                )?;
                db.complete_pending_envelope(&self.topic(), self.welcome.cursor)?;
                Ok::<_, GroupError>(Continue(()))
            })?;
        }
        Err(error)
    }

    /// Roll back trial MLS writes before resolving exact identity proofs.
    /// Only immutable input and public membership data cross the network await.
    async fn validate_membership(
        &self,
    ) -> Result<(ResolvedWelcome, WelcomeMembership), GroupError> {
        let resolved = ResolvedWelcome::resolve(self.welcome, &self.context).await?;
        let mut membership = None;
        state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let decrypted = resolved.stage(self.welcome, &storage)?;
            self.join_anchor(&decrypted)?;
            membership = Some(WelcomeMembership::from_staged(&decrypted.staged_welcome)?);
            Ok::<_, GroupError>(Rollback::<()>)
        })?;
        let membership = membership.ok_or(GroupError::UninitializedResult)?;
        membership.validate_sequences(self.welcome.sequence_id())?;
        self.validator.check_initial_membership(&membership).await?;
        Ok((resolved, membership))
    }

    /// Require an authenticated anchor before this Welcome's sequence.
    /// Zero is valid only for epoch zero or an Oneshot Welcome.
    // implements: JOIN-036, JOIN-037, JOIN-038
    fn join_anchor(&self, decrypted: &DecryptedWelcome) -> Result<Cursor, GroupError> {
        let metadata =
            extract_group_metadata(decrypted.staged_welcome.group_context().extensions())
                .map_err(MetadataPermissionsError::from)?;
        let anchor = match &decrypted.welcome_metadata {
            Some(metadata) => metadata.message_cursor,
            None if metadata.conversation_type == ConversationType::Oneshot => 0,
            None => return Err(GroupError::InvalidWelcomeMetadata),
        };
        let initial = metadata.conversation_type == ConversationType::Oneshot
            || decrypted.staged_welcome.group_context().epoch().as_u64() == 0;
        if anchor >= self.welcome.sequence_id() || (anchor == 0 && !initial) {
            return Err(GroupError::InvalidWelcomeMetadata);
        }
        Ok(Cursor(anchor))
    }

    /// Commit a valid Welcome or a safe rejection with its pending-row completion.
    /// Other failures roll back all state. Send events only after commit.
    fn commit_or_fail_forever(
        &self,
        resolved: &ResolvedWelcome,
        membership: Option<&WelcomeMembership>,
        missing_reference: Option<&IdentityRequirement>,
    ) -> Result<CommitResult<C>, GroupError> {
        tracing::debug!("attempting to commit welcome={}", &self.welcome.cursor);
        let commit_result = state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, event_buffer| {
                let storage = tx.storage();
                self.check_pending(&storage.db())?;
                // Savepoint transaction
                let result = event_buffer.savepoint(|event_buffer| {
                    storage.savepoint(|conn| {
                        self.commit(conn, resolved, membership, event_buffer)
                            .map(Continue)
                    })
                });
                let db = storage.db();
                // Only the resolver can prove that an exact identity reference is absent.
                let result = result.map_err(|error| match (&error, missing_reference) {
                    (
                        GroupError::InstallationDiff(InstallationDiffError::IdentityDependency(
                            IdentityDependencyError::Need(required),
                        )),
                        Some(missing),
                    ) if required.contains(missing) => InstallationDiffError::IdentityDependency(
                        IdentityDependencyError::MissingReference(missing.clone()),
                    )
                    .into(),
                    _ => error,
                });
                match result {
                    Err(err) if terminal_welcome_error(&err) => {
                        db.record_terminal_rejection(
                            &self.topic(),
                            self.welcome.cursor,
                            "invalid_welcome",
                        )?;
                        db.complete_pending_envelope(&self.topic(), self.welcome.cursor)?;
                        // return ok to commit the transaction
                        Ok(Continue(CommitResult::FailedForever(err)))
                    }
                    // roll everything back to retry
                    Err(e) => Err(e),
                    Ok(Continue(group)) => {
                        db.complete_pending_envelope(&self.topic(), self.welcome.cursor)?;
                        // Rotate the key package this Welcome used only once it is
                        // installed, in the same transaction. Receipt, a hold, or a
                        // rejection never queues rotation.
                        crate::worker::key_package_maintenance::queue_key_rotation_in(&storage)?;
                        event_buffer.emit(None, Some(InternalEvent::TaskScheduled));
                        self.context.task_channels().mark_notification_changed();
                        Ok(Continue(CommitResult::Ok(group)))
                    }
                    Ok(Rollback) => {
                        unreachable!("savepoint never intentionally rolls back here")
                    }
                }
            },
        )
        .map(TransactionOutcome::into_continued)?;
        Ok(commit_result)
    }

    /// Restage and recheck the join against this writer's keys, proofs, and group.
    /// An active older group must process its removal before replacement.
    /// The caller commits the join and pending-row completion together.
    // implements: JOIN-080
    fn commit(
        &self,
        tx: &mut impl TransactionalKeyStore,
        resolved: &ResolvedWelcome,
        expected_membership: Option<&WelcomeMembership>,
        event_buffer: &EventBuffer<'_, InternalEvent>,
    ) -> Result<Option<MlsGroup<C>>, GroupError> {
        let Self {
            welcome, context, ..
        } = self;

        let storage = tx.key_store();
        let db = storage.db();
        let provider = XmtpOpenMlsProviderRef::new(&storage);

        self.check_pending(&db)?;
        let decrypted = resolved.stage(welcome, &storage)?;
        let anchor = self.join_anchor(&decrypted)?;
        let membership = WelcomeMembership::from_staged(&decrypted.staged_welcome)?;
        membership.validate_sequences(welcome.sequence_id())?;
        if expected_membership.is_some_and(|expected| *expected != membership) {
            return Err(GroupError::LockUnavailable);
        }
        let DecryptedWelcome {
            staged_welcome,
            added_by_inbox_id,
            added_by_installation_id,
            welcome_metadata: _,
        } = decrypted;
        let metadata = extract_group_metadata(staged_welcome.group_context().extensions())
            .map_err(MetadataPermissionsError::from)?;
        // implements: JOIN-083
        // The root must derive the group id. No 32-byte key equals a 16-byte
        // id, so this also rejects every Welcome for a legacy group, and a
        // Welcome naming a restored placeholder activates it only when it
        // comes from a holder of the placeholder's root.
        // The commit-log id is stored with the row; the keys are wiped here.
        let commit_log_id = check_group_root(
            staged_welcome.group_context().extensions(),
            staged_welcome.group_context().group_id().as_slice(),
        )
        .map_err(GroupError::WelcomeGroupRoot)?
        .commit_log_id();
        // Reject before identity checks, which can wait on the network.
        // implements: SYNC-010
        if metadata.conversation_type == ConversationType::Sync
            && !is_own_sync_group(
                context.inbox_id(),
                &added_by_inbox_id,
                staged_welcome.members(),
            )
        {
            tracing::warn!(
                added_by_inbox_id,
                "rejecting a sync group Welcome that names another inbox"
            );
            return Err(GroupError::InvalidGroupMembership);
        }
        self.validator.check_verified_membership(&membership, &db)?;
        // implements: JOIN-089
        // A held row's conversation type and dm id never change through a
        // Welcome, for a Restored placeholder and a live group alike. This runs
        // before the Oneshot branch and the cursor and epoch gate, so a changed
        // identity is rejected at once and never held for the group's topic.
        let group_id = GroupId::try_from(staged_welcome.group_context().group_id())?;
        let existing_group = db.find_group(&group_id)?;
        if let Some(existing) = &existing_group {
            if existing.conversation_type != metadata.conversation_type {
                return Err(GroupError::InvalidWelcomeMetadata);
            }
            if existing.conversation_type == ConversationType::Dm {
                let stored_pair = crate::groups::parse_canonical_dm_id(existing.dm_id.as_deref())?;
                if metadata.dm_members.as_ref().map(String::from).as_deref()
                    != Some(stored_pair.to_string().as_str())
                {
                    return Err(MetadataPermissionsError::from(
                        crate::groups::DmValidationError::StoredDmIdMismatch,
                    )
                    .into());
                }
            }
        }
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

        let mut anchor_mode = JoinAnchorMode::Advance;

        if let Some(existing) = &existing_group
            && existing.membership_state != GroupMembershipState::Restored
        {
            let mut current = OpenMlsGroup::load(&storage, &group_id.to_openmls())?
                .ok_or(xmtp_db::NotFound::MlsGroup(group_id))?;
            let processed = db.latest_cursor_for_id(group_id, &[EntityKind::ApplicationMessage])?;
            let incoming_epoch = staged_welcome.group_context().epoch();
            let active = current.is_active();
            // A remove-and-re-add commit can retire this installation at the join anchor.
            // Removal can advance its public epoch without installing that epoch's secrets.
            // Welcome publication order does not establish MLS epoch order.
            if processed == anchor && !current.is_active() && current.epoch() <= incoming_epoch {
                anchor_mode = JoinAnchorMode::InactiveReadd;
            }
            // implements: JOIN-042
            // A lower epoch is a stale branch whatever anchor the sender chose,
            // so it never moves the stored cursor or drops queued envelopes.
            if processed > anchor
                || (processed == anchor && anchor_mode != JoinAnchorMode::InactiveReadd)
                || current.epoch() > incoming_epoch
                || (active && current.epoch() >= incoming_epoch)
            {
                return Err(ProcessIntentError::WelcomeAlreadyProcessed(welcome.cursor).into());
            }
            if active {
                return Err(GroupError::WelcomeGroupPrefixPending {
                    group_id,
                    anchor: anchor.0,
                });
            }
            // A replacement Welcome starts a new membership generation.
            // Clear the old proposals in memory and storage before installing
            // it. OpenMLS stores proposal references by group ID, not epoch.
            current.clear_pending_proposals(&storage)?;
            db.forget_received_proposals(group_id)?;
        }

        // The checks above allow PendingRemove only for a valid inactive rejoin.
        // It is not Restored, and an active MLS group returns before this point.
        let is_readd_after_leaving = existing_group
            .as_ref()
            .is_some_and(|g| g.membership_state == GroupMembershipState::PendingRemove);

        let prior_membership_cutoff = db.get_local_commit_log_cursor(&group_id)?.unwrap_or(0);
        let mls_group = OpenMlsGroup::from_welcome_logged(
            &provider,
            staged_welcome,
            &added_by_inbox_id,
            &added_by_installation_id,
            context.server_configuration().commit_log_enabled(),
        )?;
        // implements: FORK-089
        // A Welcome carries no prior publisher tenure history. Its authenticated
        // group-message anchor is the boundary for this installed membership.
        let publisher_boundary =
            i64::try_from(anchor.0).map_err(|_| xmtp_db::StorageError::DbSerialize)?;
        let publisher_boundaries = xmtp_mls_validation::ForkActors::from_group(&mls_group)?
            .publisher_keys(&mls_group)?
            .into_iter()
            .map(|author| (author, publisher_boundary))
            .collect();
        db.set_commit_log_publisher_boundaries(&group_id, &publisher_boundaries)?;
        let dm_members = metadata.dm_members;
        let conversation_type = metadata.conversation_type;
        // Required metadata must not bypass the version check.
        let mutable_metadata = Some(
            extract_group_mutable_metadata_capability_aware(&mls_group)
                .map_err(|_| GroupError::InvalidWelcomeMetadata)?,
        );
        let disappearing_settings = mutable_metadata.as_ref().and_then(|metadata| {
            MlsGroup::<C>::conversation_message_disappearing_settings_from_extensions(metadata).ok()
        });

        if let Some(min_version) = mutable_metadata
            .as_ref()
            .and_then(MlsGroup::<C>::min_protocol_version_from_extensions)
        {
            let required = LibXMTPVersion::parse(&min_version)
                .map_err(|_| GroupError::InvalidWelcomeMetadata)?;
            if required > *context.version_info().pkg_semver() {
                return Err(GroupError::UnsupportedWelcomeVersion(min_version));
            }
        }

        // A Welcome is peer state, even on a clean database. Validate action
        // entries and complete policy trees before admitting any group type.
        {
            let registry = crate::groups::app_data::load_component_registry(&mls_group)
                .map_err(|_| GroupError::InvalidWelcomeMetadata)?;
            xmtp_mls_common::app_data::policy_set::validate_registry_action_policies(&registry)
                .map_err(|error| {
                    tracing::warn!(%error, "invalid registry action policy in Welcome");
                    GroupError::InvalidWelcomeMetadata
                })?;
        }

        // Determine the membership state
        // If the user is being re-added after leaving, set to ALLOWED
        // Otherwise, new members start in PENDING state
        let membership_state = if is_readd_after_leaving {
            tracing::info!(
                group_id = %group_id,
                "User is being re-added after leaving/removal, setting membership state to ALLOWED"
            );
            GroupMembershipState::Allowed
        } else {
            tracing::debug!(
                group_id = %group_id,
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
            .commit_log_public_key(Some(commit_log_id.to_vec()))
            .should_publish_commit_log(MlsGroup::<C>::check_should_publish_commit_log(
                context.inbox_id().to_string(),
                mutable_metadata,
            ));

        let to_store = match conversation_type {
            ConversationType::Group => group.membership_state(membership_state).build()?,
            ConversationType::Dm => {
                validate_dm_group(context, &mls_group, &added_by_inbox_id)?;
                group
                    .membership_state(membership_state)
                    .last_message_ns(welcome.timestamp())
                    .build()?
            }
            ConversationType::Sync => {
                // The own inbox's sync groups are Allowed; others were rejected above.
                group
                    .membership_state(GroupMembershipState::Allowed)
                    .build()?
            }
            ConversationType::Oneshot => {
                unreachable!("StagedWelcome of type Oneshot should already be handled")
            }
        };

        tracing::debug!("storing group with welcome id {}", welcome.cursor);

        // If this is a re-add after leaving, update the existing group's membership state
        // before calling insert_or_replace_group
        if is_readd_after_leaving && let Some(ref existing) = existing_group {
            tracing::info!(
                group_id = %existing.id,
                "Updating existing group membership state from PENDING_REMOVE to ALLOWED"
            );
            db.update_group_membership(existing.id, GroupMembershipState::Allowed)?;
        }

        // implements: FORK-041
        if existing_group
            .as_ref()
            .is_some_and(|group| group.membership_state != GroupMembershipState::Restored)
        {
            db.set_group_commit_log_forked_status(&group_id, None)?;
        }

        // Insert the group, replace a Restored placeholder, or advance an existing cursor.
        let should_publish = to_store.should_publish_commit_log;
        let mut stored_group = db.insert_or_replace_group(to_store)?;
        // implements: FORK-085
        // Existing rows retain their fields on rejoin; refresh the publisher role
        // from this Welcome in the same state transaction.
        db.set_should_publish_commit_log(&stored_group.id, should_publish)?;
        // This Welcome starts a new membership even when local logging is off.
        // Exclude every older local record and durably finish legacy recovery.
        db.update_cursor(
            group_id,
            EntityKind::CommitLogUpload,
            Cursor(prior_membership_cutoff as u64),
        )?;
        db.update_cursor(group_id, EntityKind::CommitLogPublisherRecovery, Cursor(1))?;
        // This membership has an authenticated Welcome boundary instead of a legacy floor.
        db.set_commit_log_reader_epoch_floor(&group_id, 0)?;
        stored_group.should_publish_commit_log = should_publish;
        let consent_entity = hex::encode(stored_group.id);
        let prior_consent =
            db.get_consent_record(consent_entity.clone(), ConsentType::ConversationId)?;
        resolve_join_consent(
            &db,
            &stored_group,
            context.inbox_id(),
            &metadata.creator_inbox_id,
            &added_by_inbox_id,
            is_readd_after_leaving,
            welcome.timestamp(),
        )?;

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

        let added_idempotency_key = format!("{}_welcome_added", welcome.created_ns);
        let added_message_id = crate::utils::id::calculate_message_id(
            stored_group.id,
            encoded_added_payload_bytes.as_slice(),
            &added_idempotency_key,
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

        let cursor = anchor.0 as i64;

        // this is the commit that brought us into the group
        let added_msg = StoredGroupMessage {
            id: added_message_id,
            group_id: stored_group.id,
            decrypted_message_bytes: encoded_added_payload_bytes,
            sent_at_ns: welcome.timestamp(),
            kind: GroupMessageKind::MembershipChange,
            sender_installation_id: added_by_installation_id,
            sender_inbox_id: added_by_inbox_id.clone(),
            delivery_status: DeliveryStatus::Published,
            content_type: ContentType::from_identifier(
                &added_content_type.authority_id,
                &added_content_type.type_id,
                added_content_type.version_major,
            ),
            version_major: added_content_type.version_major as i32,
            version_minor: added_content_type.version_minor as i32,
            authority_id: added_content_type.authority_id,
            reference_id: None,
            sequence_id: cursor,
            envelope_hash: None,
            expiry_ns: None,
            expire_at_ns: None,
            inserted_at_ns: 0, // Will be set by database
            should_push: true,
            // Matches the key used to derive `added_message_id` above.
            idempotency_key: added_idempotency_key,
        };

        added_msg.store_or_ignore(&db)?;

        tracing::debug!("created GroupUpdated message for welcome, inbox_id={current_inbox_id}");

        let group = MlsGroup::new(
            context.clone(),
            stored_group.id,
            stored_group.dm_id,
            stored_group.conversation_type,
            stored_group.created_at_ns,
        );

        if !stored_group.conversation_type.is_virtual() {
            let final_consent =
                db.get_consent_record(consent_entity, ConsentType::ConversationId)?;
            if prior_consent.as_ref().map(|record| record.state)
                != final_consent.as_ref().map(|record| record.state)
                && let Some(record) = final_consent
            {
                // implements: SYNC-021
                // Apps see the change; device sync never publishes a join record.
                emit_preference_updates(
                    event_buffer,
                    vec![
                        crate::worker::device_sync::preference_sync::PreferenceUpdate::Consent(
                            record,
                        ),
                    ],
                    PreferenceOrigin::Derived,
                    &db,
                )?;
            }
        }

        // State, progress, and removal of pre-join work commit together.
        db.install_group_anchor(group.group_id, anchor, anchor_mode)?;
        db.record_welcome_discovery(group.group_id, welcome.cursor)?;
        MlsGroup::<C>::mark_readd_requests_as_responded(
            &storage,
            &group.group_id,
            &HashSet::from([context.installation_id().to_vec()]),
            cursor,
        )?;
        event_buffer.emit(
            (!stored_group.conversation_type.is_virtual()).then(|| {
                ClientEvent::ConversationJoined(ConversationJoined {
                    group_id: group.group_id.to_vec(),
                    conversation_type: if stored_group.conversation_type == ConversationType::Dm {
                        EventConversationType::Dm
                    } else {
                        EventConversationType::Group
                    },
                    origin: JoinOrigin::Welcomed,
                    adder_inbox_id: Some(added_by_inbox_id.clone()),
                })
            }),
            Some(InternalEvent::GroupJoined {
                group_id: group.group_id,
                is_sync: stored_group.conversation_type == ConversationType::Sync,
                origin: crate::subscriptions::internal::GroupOrigin::Welcomed,
            }),
        );

        tracing::debug!(
            inbox_id = %current_inbox_id,
            installation_id = %self.context.installation_id().as_slice().snippet(),
            group_id = %group.group_id,
            welcome_id = welcome.cursor.0,

            cursor = cursor,
            "updated message cursor from welcome metadata"
        );

        Ok(Some(group))
    }
}

/// Store the consent a join derives by rule. Every write is a merge or an
/// insert-if-absent, so a join never replaces a decision newer than the
/// record it writes.
// implements: CONS-022, CONS-045, CONS-024, CONS-025
fn resolve_join_consent(
    db: &impl DbQuery,
    group: &StoredGroup,
    own_inbox_id: &str,
    creator_inbox_id: &str,
    added_by_inbox_id: &str,
    readd_after_leaving: bool,
    welcome_ns: i64,
) -> Result<(), GroupError> {
    // Inherit first, so the rules below apply to the inherited record.
    StoredConsentRecord::stitch_dm_consent(db, group)?;
    let entity = hex::encode(group.id);
    let record = |state, consented_at_ns| StoredConsentRecord {
        entity_type: ConsentType::ConversationId,
        state,
        entity: entity.clone(),
        consented_at_ns,
    };
    // A re-add resets only a decision older than its Welcome. With no record
    // there is nothing to reset, and a join stores no record at a later time.
    if readd_after_leaving
        && db
            .get_consent_record(entity.clone(), ConsentType::ConversationId)?
            .is_some()
    {
        db.insert_newer_consent_record(record(ConsentState::Unknown, welcome_ns))?;
    }
    // The creator field is the sender's claim, so the adder must be this inbox too.
    if creator_inbox_id == own_inbox_id && added_by_inbox_id == own_inbox_id {
        db.maybe_insert_consent_record_return_existing(&record(ConsentState::Allowed, 0))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groups::InitialMembershipValidator;
    use crate::tester;
    use crate::utils::test::MlsGroupExt;

    struct UnavailableValidator;

    impl ValidateGroupMembership for UnavailableValidator {
        async fn check_initial_membership(
            &self,
            _welcome: &WelcomeMembership,
        ) -> Result<(), GroupError> {
            Err(GroupError::LockUnavailable)
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn trial_validation_failure_preserves_welcome_keys() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let alix_group = alix.create_group(None, None)?;
        alix_group.invite(&bo).await?;
        let welcome = bo
            .context
            .api()
            .query_welcome_messages(bo.context.installation_id())
            .await?
            .pop()?;

        let events = bo.context.events().subscribe(
            xmtp_events::EventFilter::new([xmtp_events::EventKind::ConversationJoined]),
            Some(10),
        );
        let result = XmtpWelcome::builder()
            .context(bo.context.clone())
            .welcome(&welcome)
            .pending(
                crate::groups::welcome_sync::pending_welcome_for_test(&bo.context, &welcome)
                    .await?,
            )
            .validator(UnavailableValidator)
            .process()
            .await;
        assert!(matches!(result, Err(GroupError::LockUnavailable)));
        assert!(events.drain().is_empty());
        assert!(bo.context.db().find_group(&alix_group.group_id)?.is_none());
        assert_eq!(
            bo.context
                .db()
                .get_last_cursor(bo.context.installation_id(), EntityKind::Welcome)?,
            Cursor(0)
        );

        let bo_group = bo.sync_welcomes().await?.pop()?;
        assert!(matches!(
            events.drain().pop().and_then(|event| event.client),
            Some(xmtp_events::ClientEvent::ConversationJoined(joined)) if joined.group_id == bo_group.group_id.to_vec()
        ));
        alix_group.test_can_talk_with(&bo_group).await?;
    }

    struct JoinDuringValidation<C> {
        context: C,
        welcome: xmtp_proto::types::WelcomeMessage,
    }

    impl<C: XmtpSharedContext> ValidateGroupMembership for JoinDuringValidation<C> {
        async fn check_initial_membership(
            &self,
            _welcome: &WelcomeMembership,
        ) -> Result<(), GroupError> {
            let group = XmtpWelcome::builder()
                .context(self.context.clone())
                .welcome(&self.welcome)
                .pending(
                    crate::groups::welcome_sync::pending_welcome_for_test(
                        &self.context,
                        &self.welcome,
                    )
                    .await?,
                )
                .validator(InitialMembershipValidator::new(self.context.clone()))
                .process()
                .await?
                .ok_or(GroupError::UninitializedResult)?;
            group.sync_with_conn().await?;
            Ok(())
        }
    }

    #[xmtp_common::test(unwrap_try = true)]
    async fn fresh_install_rejects_welcome_after_another_writer_advances_group() {
        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let alix_group = alix.create_group(None, None)?;
        alix_group.invite(&bo).await?;
        alix_group
            .update_group_name("advanced during validation".into())
            .await?;
        let welcome = bo
            .context
            .api()
            .query_welcome_messages(bo.context.installation_id())
            .await?
            .pop()?;

        let result = XmtpWelcome::builder()
            .context(bo.context.clone())
            .welcome(&welcome)
            .pending(
                crate::groups::welcome_sync::pending_welcome_for_test(&bo.context, &welcome)
                    .await?,
            )
            .validator(JoinDuringValidation {
                context: bo.context.clone(),
                welcome: welcome.clone(),
            })
            .process()
            .await;
        assert!(matches!(
            result,
            Err(GroupError::ProcessIntent(
                ProcessIntentError::WelcomeAlreadyProcessed(_)
            ))
        ));
        let bo_group = bo.group(&alix_group.group_id)?;
        assert_eq!(
            alix_group.epoch_authenticator().await?,
            bo_group.epoch_authenticator().await?
        );
        alix_group.test_can_talk_with(&bo_group).await?;
    }

    /// Process `joiner`'s newest Welcome with the production validator.
    async fn process_newest_welcome(
        joiner: &crate::utils::FullXmtpClient,
    ) -> Result<Option<MlsGroup<crate::utils::TestXmtpMlsContext>>, GroupError> {
        let welcome = joiner
            .context
            .api()
            .query_welcome_messages(joiner.context.installation_id())
            .await?
            .into_iter()
            .max_by_key(|welcome| welcome.cursor)
            .ok_or(GroupError::UninitializedResult)?;
        XmtpWelcome::builder()
            .context(joiner.context.clone())
            .welcome(&welcome)
            .pending(
                crate::groups::welcome_sync::pending_welcome_for_test(&joiner.context, &welcome)
                    .await?,
            )
            .validator(InitialMembershipValidator::new(joiner.context.clone()))
            .process()
            .await
    }

    /// A Welcome whose group context has no root, a root for another id, or a
    /// 16-byte group id is rejected for good, and its group is not installed.
    /// A one-shot Welcome with no root is rejected the same way, before its
    /// one-shot message is read. The same crafted Welcome with the group's own
    /// root joins, so the root is the only reason for each rejection.
    // verifies: JOIN-083, JOIN-048
    #[xmtp_common::test(unwrap_try = true)]
    async fn a_welcome_joins_only_when_its_root_names_the_group() {
        use crate::utils::test::crafted_welcome::{
            send_crafted_welcome, send_crafted_welcome_of_type,
        };
        use xmtp_mls_common::app_data::group_root::GroupRootError;
        use xmtp_mls_common::group_keys::{GroupKeys, GroupRoot};

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let keyed_id = |root: &GroupRoot| GroupKeys::derive(root).group_id().to_vec();
        let root = GroupRoot::generate();
        let other_root = GroupRoot::generate();
        let legacy_id = xmtp_common::rand_vec::<{ xmtp_configuration::LEGACY_GROUP_ID_BYTES }>();
        let topic = StreamTopic {
            entity_id: bo.context.installation_id().to_vec(),
            kind: NetworkEntityKind::Welcome,
        };

        for (case, group_id, welcome_root, conversation_type, expected) in [
            (
                "no root",
                keyed_id(&root),
                None,
                ConversationType::Group,
                GroupRootError::Missing,
            ),
            (
                "another group's root",
                keyed_id(&root),
                Some(&other_root),
                ConversationType::Group,
                GroupRootError::OtherGroup,
            ),
            (
                "a 16-byte group",
                legacy_id.clone(),
                Some(&root),
                ConversationType::Group,
                GroupRootError::OtherGroup,
            ),
            (
                "a one-shot group with no root",
                keyed_id(&other_root),
                None,
                ConversationType::Oneshot,
                GroupRootError::Missing,
            ),
        ] {
            send_crafted_welcome_of_type(&alix, &bo, &group_id, welcome_root, conversation_type)
                .await;
            let cursor = bo
                .context
                .api()
                .query_welcome_messages(bo.context.installation_id())
                .await?
                .into_iter()
                .map(|welcome| welcome.cursor)
                .max()
                .expect(case);
            let result = process_newest_welcome(&bo).await;
            assert!(
                matches!(&result, Err(GroupError::WelcomeGroupRoot(error)) if *error == expected),
                "{case}: {result:?}"
            );
            let group_id = GroupId::try_from(group_id.as_slice())?;
            assert!(bo.context.db().find_group(&group_id)?.is_none(), "{case}");
            let rejected = bo.context.db().read_last_rejection(&topic)?.expect(case);
            assert_eq!(
                rejected.sequence_id, cursor,
                "{case}: the rejection is this Welcome's"
            );
            assert_eq!(rejected.code, "invalid_welcome", "{case}");
            assert!(
                bo.context.db().pending_envelope(&topic, cursor)?.is_none(),
                "{case}: the rejected Welcome must not stay pending"
            );
        }

        let joined_id = keyed_id(&root);
        send_crafted_welcome(&alix, &bo, &joined_id, Some(&root)).await;
        let joined = process_newest_welcome(&bo).await?.expect("a group");
        assert_eq!(joined.group_id.as_slice(), joined_id.as_slice());
    }

    fn welcome_topic(client: &crate::utils::FullXmtpClient) -> StreamTopic {
        StreamTopic {
            entity_id: client.context.installation_id().to_vec(),
            kind: NetworkEntityKind::Welcome,
        }
    }

    /// Assert that `client` rejected its newest Welcome for good.
    async fn assert_newest_welcome_rejected(client: &crate::utils::FullXmtpClient) {
        let newest = client
            .context
            .api()
            .query_welcome_messages(client.context.installation_id())
            .await
            .unwrap()
            .into_iter()
            .map(|welcome| welcome.cursor)
            .max()
            .expect("a published Welcome");
        let rejected = client
            .context
            .db()
            .read_last_rejection(&welcome_topic(client))
            .unwrap()
            .expect("a terminal rejection");
        assert_eq!(rejected.sequence_id, newest);
        assert_eq!(rejected.code, "invalid_welcome");
    }

    /// A former member can offer a second MLS branch under the group's root,
    /// at a lower epoch than the removed copy and with an anchor past the
    /// removal. It is rejected for good, and the removed copy keeps its
    /// epoch, its cursor, and the envelopes queued after the removal.
    // verifies: JOIN-042
    #[xmtp_common::test(unwrap_try = true)]
    async fn inactive_group_rejects_stale_branch_and_keeps_pending_prefix() {
        use crate::utils::test::crafted_welcome::send_crafted_welcome_with_anchor;
        use xmtp_db::incoming_envelope::QueryIncomingEnvelope;
        use xmtp_mls_common::app_data::group_root::read_group_root;
        use xmtp_proto::types::{OrderedEnvelopeBatch, Topic};

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let alix_group = alix.create_group(None, None)?;
        let root = read_group_root(alix_group.load_group_context()?.extensions())?
            .expect("a new group carries its root");
        alix_group.invite(&bo).await?;
        let bo_group = bo.sync_welcomes().await?.pop().expect("initial Welcome");
        bo_group.sync().await?;
        alix_group.remove_members(&[bo.inbox_id()]).await?;
        let _ = bo_group.sync().await;
        assert!(!bo_group.is_active()?, "the removal must be applied");

        let db = bo.context.db();
        let topic = StreamTopic::group(bo_group.group_id);
        let before = db.topic_progress(&topic)?;
        let old_epoch = bo_group.epoch().await?;
        assert!(old_epoch > 1, "the crafted branch joins at epoch one");

        // Admit a real group envelope after the removal without processing it.
        // The forged anchor names this pending network position.
        alix_group.send_msg(b"after removal").await;
        let wire_topic = Topic::new_group_message(bo_group.group_id);
        let envelopes = bo
            .context
            .api()
            .query_all(
                [(wire_topic.clone(), before.received)].into(),
                bo.context.api().limits().max_query_limit as u32,
            )
            .await?;
        assert_eq!(envelopes.len(), 1);
        bo.mls_store().admit_incoming_batch(
            &OrderedEnvelopeBatch {
                topic: wire_topic,
                after: before.received,
                envelopes,
            },
            bo.context
                .incoming_runtime()
                .policy()
                .incoming_limits(topic.kind),
        )?;
        let forged_anchor = db.topic_progress(&topic)?.received;
        assert!(before.processed < forged_anchor);
        assert!(db.pending_envelope(&topic, forged_anchor)?.is_some());

        send_crafted_welcome_with_anchor(
            &alix,
            &bo,
            alix_group.group_id.as_slice(),
            &root,
            forged_anchor.0,
        )
        .await;
        let result = process_newest_welcome(&bo).await;
        assert!(
            matches!(
                result,
                Err(GroupError::ProcessIntent(
                    ProcessIntentError::WelcomeAlreadyProcessed(_)
                ))
            ),
            "{result:?}"
        );
        assert_newest_welcome_rejected(&bo).await;
        assert!(!bo_group.is_active()?);
        assert_eq!(bo_group.epoch().await?, old_epoch);
        assert_eq!(db.topic_progress(&topic)?.processed, before.processed);
        assert!(db.pending_envelope(&topic, forged_anchor)?.is_some());
    }

    /// A removed former member reuses the group's root to offer a DM under
    /// the group's id. The Welcome is rejected for good before anything is
    /// installed: the stored row, MLS state, and cursor stay as they were, and
    /// the removed group still cannot send.
    // verifies: JOIN-089
    #[xmtp_common::test(unwrap_try = true)]
    async fn replacement_welcome_cannot_change_conversation_type() {
        use crate::utils::test::crafted_welcome::send_crafted_dm_welcome_with_peer;
        use xmtp_mls_common::app_data::group_root::read_group_root;

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(caro, disable_workers);

        let original = alix.create_group(None, None)?;
        original.invite(&caro).await?;
        let caro_group = caro.sync_welcomes().await?.pop().expect("Caro joined");
        let retained_root = read_group_root(caro_group.load_group_context()?.extensions())?
            .expect("the original group has a root");
        original.invite(&bo).await?;
        let bo_group = bo.sync_welcomes().await?.pop().expect("Bo joined");
        original.remove_members(&[caro.inbox_id()]).await?;
        original.remove_members(&[bo.inbox_id()]).await?;
        bo_group.sync_with_conn().await?;
        assert!(!bo_group.is_active()?);
        let row_before_forgery = bo
            .context
            .db()
            .find_group(&original.group_id)?
            .expect("stored");
        assert_eq!(
            row_before_forgery.conversation_type,
            ConversationType::Group
        );
        let anchor = bo_group.cursor().await?;
        let prior_epoch = bo_group.epoch().await?;

        // Caro's DM with Bo, at the next epoch and anchored at Bo's cursor,
        // passes every check a same-type re-add Welcome would.
        send_crafted_dm_welcome_with_peer(
            &caro,
            &bo,
            original.group_id.as_slice(),
            &retained_root,
            bo.inbox_id(),
            anchor.0,
            prior_epoch,
        )
        .await;
        let result = process_newest_welcome(&bo).await;
        assert!(
            matches!(result, Err(GroupError::InvalidWelcomeMetadata)),
            "{result:?}"
        );
        assert_newest_welcome_rejected(&bo).await;
        assert_eq!(
            bo.context.db().find_group(&original.group_id)?,
            Some(row_before_forgery)
        );
        assert!(!bo_group.is_active()?);
        assert_eq!(bo_group.epoch().await?, prior_epoch);
        assert_eq!(bo_group.cursor().await?, anchor);
        assert!(
            bo_group
                .send_message(b"for the original group", Default::default())
                .await
                .is_err()
        );
        assert!(
            bo.context
                .api()
                .query_group_messages(original.group_id)
                .await?
                .iter()
                .all(|message| message.cursor <= anchor),
            "nothing is published after the forged Welcome"
        );
    }

    /// A member of a live DM holds its root and can offer a DM under its id
    /// with another pair. The client rejects it for good at once instead of
    /// holding it for the DM's topic, and keeps the DM it holds.
    // verifies: JOIN-089
    #[xmtp_common::test(unwrap_try = true)]
    async fn live_dm_rejects_welcome_with_another_pair() {
        use crate::groups::DmValidationError;
        use crate::utils::test::crafted_welcome::send_crafted_dm_welcome_with_peer;
        use xmtp_mls_common::app_data::group_root::read_group_root;

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        tester!(caro, disable_workers);
        let alix_dm = alix.find_or_create_dm(bo.inbox_id(), None).await?;
        let bo_dm = bo.sync_welcomes().await?.pop().expect("Bo joined the DM");
        assert!(bo_dm.is_active()?);
        let root = read_group_root(alix_dm.load_group_context()?.extensions())?
            .expect("a new DM carries its root");
        let row_before = bo.context.db().find_group(&alix_dm.group_id)?;
        let cursor_before = bo_dm.cursor().await?;
        let epoch_before = bo_dm.epoch().await?;

        send_crafted_dm_welcome_with_peer(
            &alix,
            &bo,
            alix_dm.group_id.as_slice(),
            &root,
            caro.inbox_id(),
            1,
            0,
        )
        .await;
        let result = process_newest_welcome(&bo).await;
        assert!(
            matches!(
                result,
                Err(GroupError::MetadataPermissionsError(
                    MetadataPermissionsError::DmValidation(DmValidationError::StoredDmIdMismatch)
                ))
            ),
            "{result:?}"
        );
        assert_newest_welcome_rejected(&bo).await;
        assert_eq!(bo.context.db().find_group(&alix_dm.group_id)?, row_before);
        assert_eq!(bo_dm.cursor().await?, cursor_before);
        assert_eq!(bo_dm.epoch().await?, epoch_before);
        alix_dm.test_can_talk_with(&bo_dm).await?;
    }

    /// Another inbox can add this installation to a group whose creator field
    /// names this inbox. The join keeps a decision that reached this
    /// installation first, and stores no decision when there was none.
    // verifies: CONS-045, CONS-022
    #[rstest::rstest]
    #[case::prior_denied(true)]
    #[case::no_record(false)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn forged_creator_claim_keeps_prior_consent(
        #[case] prior_denied: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use crate::utils::test::crafted_welcome::send_crafted_welcome_with_creator_claim;
        use xmtp_mls_common::group_keys::{GroupKeys, GroupRoot};

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let root = GroupRoot::generate();
        let group_id = GroupKeys::derive(&root).group_id().to_vec();
        let entity = hex::encode(&group_id);
        // A peer installation may sync a decision before this Welcome.
        let prior = prior_denied.then(|| {
            StoredConsentRecord::new(
                ConsentType::ConversationId,
                ConsentState::Denied,
                entity.clone(),
            )
        });
        if let Some(prior) = &prior {
            bo.context.db().insert_newer_consent_record(prior.clone())?;
        }

        send_crafted_welcome_with_creator_claim(&alix, &bo, &group_id, &root, bo.inbox_id()).await;
        let joined = process_newest_welcome(&bo).await?.expect("valid Welcome");
        assert_eq!(joined.added_by_inbox_id()?, alix.inbox_id());
        let stored = bo
            .context
            .db()
            .get_consent_record(entity, ConsentType::ConversationId)?;
        assert_eq!(
            stored.map(|record| (record.state, record.consented_at_ns)),
            prior.map(|record| (record.state, record.consented_at_ns))
        );
        Ok(())
    }

    /// A group this inbox created, joined from a Welcome another installation
    /// of this inbox sent, is allowed at consent time 0. Apps see one consent
    /// change, and device sync is not asked to publish it.
    // verifies: CONS-045, SYNC-021
    #[xmtp_common::test(unwrap_try = true)]
    async fn own_inbox_join_default_is_allowed_at_time_zero_and_unpublished() {
        use xmtp_events::{EventFilter, EventKind};

        tester!(alix1, disable_workers);
        let group = alix1.create_group(None, None)?;
        tester!(alix2, from: alix1, disable_workers);
        group.update_installations().await?;
        let events = alix2.context.events().subscribe(
            EventFilter::new([EventKind::ConsentChanged])
                .with_internal(|event| matches!(event, InternalEvent::PreferencesChanged { .. })),
            None,
        );

        alix2.sync_welcomes().await?;
        let record = alix2
            .context
            .db()
            .get_consent_record(hex::encode(group.group_id), ConsentType::ConversationId)?
            .expect("the join default");
        assert_eq!(
            (record.state, record.consented_at_ns),
            (ConsentState::Allowed, 0)
        );
        let events = events.drain();
        assert_eq!(
            events
                .iter()
                .filter(|event| matches!(event.client, Some(ClientEvent::ConsentChanged(_))))
                .count(),
            1
        );
        assert!(!events.iter().any(|event| matches!(
            event.internal,
            Some(InternalEvent::PreferencesChanged {
                origin: PreferenceOrigin::Local,
                ..
            })
        )));
    }

    // verifies: JOIN-044
    #[rstest::rstest]
    #[case::late_active_prefix(false)]
    #[case::live_retired_controller(true)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn rejoin_keeps_messages_before_removal(#[case] live_receiver: bool) {
        use crate::subscriptions::incoming::{
            IncomingCoordinator, IncomingRegistration, IncomingScope,
        };
        use xmtp_common::time::{Duration, timeout};

        tester!(alix, disable_workers);
        tester!(bo, disable_workers);
        let alix_group = alix.create_group(None, None).unwrap();
        alix_group.invite(&bo).await.unwrap();
        let bo_group = bo.sync_welcomes().await.unwrap().pop().unwrap();
        let lease = live_receiver.then(|| {
            IncomingCoordinator::for_context(&bo.context).acquire(IncomingScope::AllGroups)
        });
        let topic = xmtp_proto::types::Topic::new_group_message(bo_group.group_id);
        alix_group.send_msg(b"before removal").await;
        alix_group.remove_members(&[bo.inbox_id()]).await.unwrap();
        if let Some(lease) = &lease {
            timeout(Duration::from_secs(10), async {
                loop {
                    if lease.snapshot().topics.iter().any(|entry| {
                        entry.topic == topic && entry.registration == IncomingRegistration::Removed
                    }) {
                        break;
                    }
                    lease.changed().await;
                }
            })
            .await
            .unwrap();
        }
        alix_group.invite(&bo).await.unwrap();

        bo.sync_welcomes().await.unwrap();
        let messages = bo
            .context
            .db()
            .get_group_messages(&bo_group.group_id, &Default::default())
            .unwrap();
        assert!(
            messages
                .iter()
                .any(|message| message.decrypted_message_bytes == b"before removal")
        );
        assert_eq!(
            alix_group.epoch_authenticator().await.unwrap(),
            bo_group.epoch_authenticator().await.unwrap()
        );
        if let Some(lease) = &lease {
            alix_group.send_msg(b"after rejoin").await;
            timeout(Duration::from_secs(10), async {
                loop {
                    let messages = bo
                        .context
                        .db()
                        .get_group_messages(&bo_group.group_id, &Default::default())
                        .unwrap();
                    if messages
                        .iter()
                        .any(|message| message.decrypted_message_bytes == b"after rejoin")
                    {
                        return Ok::<_, xmtp_db::StorageError>(());
                    }
                    lease.changed().await;
                }
            })
            .await
            .unwrap()
            .unwrap();
        }
        alix_group.test_can_talk_with(&bo_group).await.unwrap();
    }
}
