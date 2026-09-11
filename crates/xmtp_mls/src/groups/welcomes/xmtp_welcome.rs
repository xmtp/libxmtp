//! XMTP Welcome Processing
//! Processes a new welcome from the network

use std::collections::HashSet;

use crate::groups::mls_ext::CommitLogStorer;
use crate::groups::mls_ext::ResolvedWelcome;
use crate::groups::mls_sync::DeferredEvents;
use crate::groups::oneshot::Oneshot;
use crate::groups::welcomes::WelcomeMembership;
use crate::groups::{MetadataPermissionsError, mls_sync};
use crate::identity_updates::{
    IdentityDependencyError, IdentityRequirement, InstallationDiffError,
};
use crate::state_tx::state_write;
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
use xmtp_common::time::now_ns;
use xmtp_content_types::ContentCodec;
use xmtp_content_types::group_updated::GroupUpdatedCodec;
use xmtp_db::TransactionOutcome::{Continue, Rollback};
use xmtp_db::{
    TransactionOutcome, XmtpOpenMlsProviderRef,
    consent_record::{ConsentState, StoredConsentRecord},
    group::{ConversationType, GroupMembershipState, StoredGroup},
    group_message::{DeliveryStatus, GroupMessageKind, StoredGroupMessage},
    incoming_envelope::{JoinAnchorMode, NetworkEntityKind, StoredIncomingEnvelope, StreamTopic},
    prelude::*,
    refresh_state::EntityKind,
};
use xmtp_mls_common::group_metadata::extract_group_metadata;

use crate::groups::app_data::component_source::extract_group_mutable_metadata_capability_aware;
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
    /// Events sent only after a successful join transaction commits.
    #[builder(default = "Some(mls_sync::DeferredEvents::default())")]
    events: Option<mls_sync::DeferredEvents>,
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
        let mut this = self.build()?;
        this.check_pending(&this.context.db())?;

        let (resolved, membership) = match this.validate_membership().await {
            Err(error) => return this.reject_or_retry(error),
            Ok(validated) => validated,
        };
        // we only use take once
        let mut events = this
            .events
            .take()
            .expect("builder is built with events as Some");
        let commit_result =
            this.commit_or_fail_forever(&resolved, Some(&membership), None, &mut events)?;
        commit_result.into_result()
    }

    /// Restage under the writer and return missing dependencies to the scheduler.
    /// Only a matching resolver result can prove an identity reference is absent.
    pub(crate) fn process_resolved(
        self,
        resolved: &ResolvedWelcome,
        missing_reference: Option<&IdentityRequirement>,
    ) -> Result<Option<MlsGroup<C>>, GroupError> {
        let mut this = self.build()?;
        let mut events = this.events.take().unwrap_or_default();
        this.commit_or_fail_forever(resolved, None, missing_reference, &mut events)?
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
    fn join_anchor(&self, decrypted: &DecryptedWelcome) -> Result<Cursor, GroupError> {
        let metadata = extract_group_metadata(
            decrypted
                .staged_welcome
                .public_group()
                .group_context()
                .extensions(),
        )
        .map_err(MetadataPermissionsError::from)?;
        let anchor = match &decrypted.welcome_metadata {
            Some(metadata) => metadata.message_cursor,
            None if metadata.conversation_type == ConversationType::Oneshot => 0,
            None => return Err(GroupError::InvalidWelcomeMetadata),
        };
        let initial = metadata.conversation_type == ConversationType::Oneshot
            || decrypted
                .staged_welcome
                .public_group()
                .group_context()
                .epoch()
                .as_u64()
                == 0;
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
        events: &mut DeferredEvents,
    ) -> Result<CommitResult<C>, GroupError> {
        tracing::debug!("attempting to commit welcome={}", &self.welcome.cursor);
        let mut attempt_events = DeferredEvents::default();
        let commit_result = state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            self.check_pending(&storage.db())?;
            // Savepoint transaction
            let result = storage.savepoint(|conn| {
                self.commit(conn, &mut attempt_events, resolved, membership)
                    .map(Continue)
            });
            let db = storage.db();
            // Only the resolver can prove that an exact identity reference is absent.
            let result = result.map_err(|error| match (&error, missing_reference) {
                (
                    GroupError::InstallationDiff(InstallationDiffError::IdentityDependency(
                        IdentityDependencyError::Need(required),
                    )),
                    Some(missing),
                ) if required == missing => InstallationDiffError::IdentityDependency(
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
                    Ok(Continue(CommitResult::Ok(group)))
                }
                Ok(Rollback) => {
                    unreachable!("savepoint never intentionally rolls back here")
                }
            }
        })
        .map(TransactionOutcome::into_continued)?;
        if matches!(&commit_result, CommitResult::Ok(_)) {
            attempt_events.send_all(&self.context);
            events.send_all(&self.context);
        }
        Ok(commit_result)
    }

    /// Restage and recheck the join against this writer's keys, proofs, and group.
    /// An active older group must process its removal before replacement.
    /// The caller commits the join and pending-row completion together.
    fn commit(
        &self,
        tx: &mut impl TransactionalKeyStore,
        events: &mut DeferredEvents,
        resolved: &ResolvedWelcome,
        expected_membership: Option<&WelcomeMembership>,
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
        self.validator.check_verified_membership(&membership, &db)?;
        let DecryptedWelcome {
            staged_welcome,
            added_by_inbox_id,
            added_by_installation_id,
            welcome_metadata: _,
        } = decrypted;
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
        let group_id = GroupId::try_from(staged_welcome.public_group().group_id())?;
        let existing_group = db.find_group(&group_id)?;
        let mut anchor_mode = JoinAnchorMode::Advance;

        if let Some(existing) = &existing_group {
            let current = OpenMlsGroup::load(&storage, &group_id.to_openmls())?
                .ok_or(xmtp_db::NotFound::MlsGroup(group_id))?;
            let processed = db.latest_cursor_for_id(group_id, &[EntityKind::ApplicationMessage])?;
            let incoming_epoch = staged_welcome.public_group().group_context().epoch();
            let active =
                current.is_active() && existing.membership_state != GroupMembershipState::Restored;
            // A remove-and-re-add commit can retire this installation at the join anchor.
            // Removal can advance its public epoch without installing that epoch's secrets.
            // Welcome publication order does not establish MLS epoch order.
            if processed == anchor && !current.is_active() && current.epoch() <= incoming_epoch {
                anchor_mode = JoinAnchorMode::InactiveReadd;
            }
            if processed > anchor
                || (processed == anchor && anchor_mode != JoinAnchorMode::InactiveReadd)
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
        }

        // The checks above allow PendingRemove only for a valid inactive rejoin.
        // It is not Restored, and an active MLS group returns before this point.
        let is_readd_after_leaving = existing_group
            .as_ref()
            .is_some_and(|g| g.membership_state == GroupMembershipState::PendingRemove);

        let mls_group = OpenMlsGroup::from_welcome_logged(
            &provider,
            staged_welcome,
            &added_by_inbox_id,
            &added_by_installation_id,
        )?;
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
            sender_inbox_id: added_by_inbox_id,
            delivery_status: DeliveryStatus::Published,
            content_type: added_content_type.type_id.into(),
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

        // If this group is created by us - auto-consent to it.
        if context.inbox_id() == metadata.creator_inbox_id {
            group.quietly_update_consent_state(ConsentState::Allowed, &db)?;
        } else if is_readd_after_leaving {
            // If user is being re-added after leaving, reset consent to Unknown
            // This requires the user to explicitly accept being added back
            tracing::info!(
                group_id = %group.group_id,
                "Resetting consent state to Unknown for re-added user"
            );
            group.quietly_update_consent_state(ConsentState::Unknown, &db)?;
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
        events.add_local_event(crate::subscriptions::LocalEvents::NewGroup(group.group_id));

        tracing::debug!(
            inbox_id = %current_inbox_id,
            installation_id = %self.context.installation_id(),
            group_id = %group.group_id,
            welcome_id = welcome.cursor.0,

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

        let mut events = bo.context.local_events().subscribe();
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
        assert!(matches!(
            events.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Empty)
        ));
        assert!(bo.context.db().find_group(&alix_group.group_id)?.is_none());
        assert_eq!(
            bo.context
                .db()
                .get_last_cursor(bo.context.installation_id(), EntityKind::Welcome)?,
            Cursor(0)
        );

        let bo_group = bo.sync_welcomes().await?.pop()?;
        assert!(matches!(
            events.try_recv()?,
            crate::subscriptions::LocalEvents::NewGroup(id) if id == bo_group.group_id
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

    // Is async so that the async timeout from rstest is used in wasm (does not spawn thread)
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn welcome_builds_with_default_events(context: NewMockContext) {
        let w = xmtp_proto::types::WelcomeMessage::generate();
        let builder = XmtpWelcome::builder()
            .context(context)
            .welcome(&w)
            .pending(StoredIncomingEnvelope {
                entity_id: Vec::new(),
                entity_kind: EntityKind::Welcome,
                sequence_id: w.cursor.0 as i64,
                envelope: Vec::new(),
                retry_at_ns: 0,
                blocked: false,
                error_code: None,
                retry_expires_at_ns: None,
            })
            .validator(NoopValidator)
            .build();
        assert!(builder.unwrap().events.is_some());
    }
}
