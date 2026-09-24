//! Envelope metadata, app-data slots, and metadata updates.

use super::*;
use xmtp_mls_validation::commit::CommitRuleError;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    pub(super) fn get_message_expire_at_ns(mls_group: &OpenMlsGroup) -> Option<i64> {
        // A parse failure here must not look identical to "disappearing
        // messages disabled" — warn before treating it as None.
        let mutable_metadata =
            xmtp_mls_common::app_data::component_source::extract_group_mutable_metadata_capability_aware(
                mls_group,
            )
            .inspect_err(|err| {
                tracing::warn!(
                    group_id = hex::encode(mls_group.group_id().as_slice()),
                    "failed to extract mutable metadata for message expiry: {err:?}"
                )
            })
            .ok()?;
        let group_disappearing_settings =
            Self::conversation_message_disappearing_settings_from_extensions(&mutable_metadata)
                .inspect_err(|err| {
                    tracing::warn!(
                        group_id = hex::encode(mls_group.group_id().as_slice()),
                        "failed to parse disappearing-message settings: {err:?}"
                    )
                })
                .ok()?;

        if group_disappearing_settings.is_enabled() {
            Some(now_ns() + group_disappearing_settings.in_ns)
        } else {
            None
        }
    }

    /// Store backend metadata without clearing fields absent from this envelope.
    pub(super) fn save_envelope_metadata_with_db(
        db: &impl DbQuery,
        envelope: &GroupMessage,
    ) -> Result<(), GroupMessageProcessingError> {
        if envelope.envelope_hash.is_none() && envelope.expiry_ns.is_none() {
            return Ok(());
        }
        use xmtp_db::diesel::prelude::*;
        use xmtp_db::schema::group_messages::dsl;
        let expiry_ns = envelope
            .expiry_ns
            .map(i64::try_from)
            .transpose()
            .map_err(|_| xmtp_proto::ConversionError::Unspecified("expiry_ns exceeds i64"))?;
        db.raw_query(|conn| {
            xmtp_db::diesel::update(dsl::group_messages)
                .filter(dsl::group_id.eq(envelope.group_id.as_slice()))
                .filter(dsl::sequence_id.eq(envelope.cursor.0 as i64))
                .set((
                    envelope
                        .envelope_hash
                        .as_ref()
                        .map(|hash| dsl::envelope_hash.eq(hash)),
                    expiry_ns.map(|expiry| dsl::expiry_ns.eq(expiry)),
                ))
                .execute(conn)
        })?;
        Ok(())
    }

    /// Record a supported invalid envelope after its trial state was discarded.
    /// The outer writer is still held, so the prefix cannot change between the
    /// failed attempt and this fresh load.
    // implements: GMOD-036
    pub(super) fn record_rejected_message(
        &self,
        group: &mut OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        envelope: &GroupMessage,
        error: &GroupMessageProcessingError,
        event_writer: &impl EventWriter<crate::subscriptions::internal::InternalEvent>,
    ) -> Result<(), GroupMessageProcessingError> {
        let db = storage.db();
        if !group.is_active() {
            return Ok(());
        }
        if !self.maybe_update_cursor(&db, envelope)? {
            return Ok(());
        }
        // implements: FORK-073
        // A competing commit can arrive after another commit advanced the epoch.
        // That stale rejection alone is not evidence of a fork.
        let suspicious_epoch = envelope.message.epoch().as_u64() >= group.epoch().as_u64();
        if matches!(error, GroupMessageProcessingError::FutureEpoch(..))
            || (suspicious_epoch
                && matches!(
                    error,
                    GroupMessageProcessingError::OpenMlsProcessMessage(
                        ProcessMessageError::ValidationError(ValidationError::WrongEpoch)
                    ) | GroupMessageProcessingError::OpenMlsProcessMessageWithAppData(
                        crate::groups::app_data::ProcessMessageWithAppDataError::OpenMls(
                            ProcessMessageError::ValidationError(ValidationError::WrongEpoch)
                        )
                    )
                ))
        {
            db.mark_group_as_maybe_forked(
                &self.group_id,
                format!(
                    "Message epoch mismatch at sequence {}",
                    envelope.sequence_id()
                ),
            )?;
            #[cfg(any(test, feature = "test-utils"))]
            tracing::warn_span!(
                "diagnostic.epoch_mismatch",
                group_id = %self.group_id.short_hex(),
                sequence_id = envelope.sequence_id(),
                message_epoch = envelope.message.epoch().as_u64(),
                current_epoch = group.epoch().as_u64(),
                is_commit = envelope.is_commit(),
                error_code = error.processing_code(),
            )
            .in_scope(|| tracing::warn!("group marked maybe_forked after an epoch mismatch"));
        }
        if envelope.is_commit() {
            if matches!(
                error,
                GroupMessageProcessingError::CommitValidation(CommitValidationError::Rule(
                    CommitRuleError::InsufficientPermissions
                ))
            ) {
                // The trial rolled back, including proposal removal. A rejected
                // combination must not remain available for the next commit.
                // A rejected commit can also contain valid concurrent app-data
                // proposals. They are evicted with the rejected combination,
                // so their authors must re-queue them.
                let own_intent = db.find_group_intent_by_payload_hash(&envelope.payload_hash)?;
                let staged_commit = if let Some(bytes) = own_intent
                    .as_ref()
                    .filter(|intent| intent.group_id == self.group_id)
                    .and_then(|intent| intent.staged_commit.as_ref())
                {
                    match decode_staged_commit(bytes) {
                        Ok(commit) => Some(commit),
                        Err(error) => {
                            tracing::warn!(
                                group_id = %self.group_id.short_hex(),
                                sequence_id = envelope.sequence_id(),
                                error = ?error,
                                "failed to decode rejected staged commit; skipping proposal cleanup",
                            );
                            None
                        }
                    }
                } else {
                    match crate::groups::app_data::process_message_with_app_data(
                        group,
                        &XmtpOpenMlsProviderRef::new(storage),
                        envelope.message.clone(),
                        self.context.version_info().pkg_semver(),
                    ) {
                        Ok(processed) => match processed.into_content() {
                            ProcessedMessageContent::StagedCommitMessage(commit) => Some(*commit),
                            _ => {
                                tracing::warn!(
                                    group_id = %self.group_id.short_hex(),
                                    sequence_id = envelope.sequence_id(),
                                    "rejected commit did not produce staged state; skipping proposal cleanup",
                                );
                                None
                            }
                        },
                        Err(error) => {
                            tracing::warn!(
                                group_id = %self.group_id.short_hex(),
                                sequence_id = envelope.sequence_id(),
                                error = ?error,
                                "failed to process rejected commit with app data; skipping proposal cleanup",
                            );
                            None
                        }
                    }
                };
                if let Some(staged_commit) = staged_commit {
                    // A commit can carry a stored proposal inline. Match its
                    // authenticated sender and payload as well as its reference.
                    let rejected_refs: Vec<_> = group
                        .pending_proposals()
                        .filter(|pending| {
                            matches!(
                                pending.proposal(),
                                openmls::prelude::Proposal::AppDataUpdate(_)
                            ) && staged_commit.queued_proposals().any(|committed| {
                                committed.proposal_reference_ref()
                                    == pending.proposal_reference_ref()
                                    || (committed.sender() == pending.sender()
                                        && committed.proposal() == pending.proposal())
                            })
                        })
                        .map(|proposal| proposal.proposal_reference_ref().clone())
                        .collect();
                    if !rejected_refs.is_empty() {
                        tracing::warn!(
                            group_id = %self.group_id.short_hex(),
                            sequence_id = envelope.sequence_id(),
                            proposal_refs = ?rejected_refs
                                .iter()
                                .map(|reference| reference.as_slice().short_hex())
                                .collect::<Vec<_>>(),
                            "evicting app-data proposals from rejected commit; authors must re-queue them",
                        );
                    }
                    for reference in rejected_refs {
                        match group.remove_pending_proposal(storage, &reference) {
                            Ok(()) | Err(openmls::group::RemoveProposalError::ProposalNotFound) => {
                            }
                            Err(openmls::group::RemoveProposalError::Storage(error)) => {
                                return Err(error.into());
                            }
                        }
                    }
                }
            }
            group.mark_failed_commit_logged(
                &XmtpOpenMlsProviderRef::new(storage),
                envelope.sequence_id(),
                envelope.message.epoch(),
                error,
                self.context.server_configuration().commit_log_enabled(),
            )?;
        }
        if let Some(intent) = db.find_group_intent_by_payload_hash(&envelope.payload_hash)?
            && intent.group_id == self.group_id
            && matches!(
                intent.state,
                IntentState::Published | IntentState::ToPublish
            )
        {
            if matches!(error, GroupMessageProcessingError::OldEpoch(..)) {
                db.set_group_intent_to_publish(intent.id)?;
            } else {
                Self::record_intent_rejection(&db, &intent, envelope, error)?;
                let message_id = calculate_message_id_for_intent(&intent)?;
                let previous_status = message_id
                    .as_ref()
                    .map(|id| db.get_group_message(id))
                    .transpose()?
                    .flatten()
                    .map(|message| message.delivery_status);
                db.set_group_intent_error_and_fail_msg(&intent, message_id.clone())?;
                if previous_status == Some(DeliveryStatus::Unpublished)
                    && let Some(id) = message_id
                {
                    self.emit_message_status_changed(
                        id,
                        xmtp_events::MessageStatus::Unpublished,
                        xmtp_events::MessageStatus::Failed,
                        event_writer,
                    );
                }
            }
        }
        tracing::warn!(
            group_id = %self.group_id.short_hex(),
            sequence_id = envelope.sequence_id(),
            reason = %error,
            "rejected ordered group envelope",
        );
        Ok(())
    }

    /// Hand observed app-data changes to the host's registered callback.
    ///
    /// Call this after the state writer and sync lock are released. The host
    /// can publish a merged result to the same group and must reacquire them.
    ///
    /// Awaited in order, one at a time: a merge decides what to write from the
    /// value it was handed, so overlapping or reordered dispatches would let a
    /// host publish a merge derived from state that has already moved on. That
    /// ordering requirement is why a slow callback cannot simply be spawned off
    /// to the side.
    ///
    /// Each callback gets `app_data_timeout` to return. Expiry is logged and
    /// never surfaced as an error: the change being reported is already durably
    /// committed, so failing the sync would turn a badly behaved host callback
    /// into a broken client.
    ///
    /// Abandoning at the budget is what *bounds* the stall, not what ends the
    /// host's work — on a binding that cannot cancel, the abandoned handler may
    /// still publish later, out of order. Restoring strict ordering would mean
    /// holding the next dispatch until the previous one truly finished, which
    /// is the unbounded wait the budget exists to prevent. The guard on
    /// `update_app_data` is the intended remedy; see
    /// [`crate::groups::change_callbacks`].
    pub(crate) async fn dispatch_app_data_changes(&self, changes: Vec<AppDataChange>) {
        let callbacks = self.context.change_callbacks();
        let Some(callback) = callbacks.app_data.clone() else {
            return;
        };
        let budget = callbacks.app_data_timeout;
        let total = changes.len();

        for (index, change) in changes.into_iter().enumerate() {
            if xmtp_common::time::timeout(budget, callback.on_app_data_changed(change))
                .await
                .is_err()
            {
                // One expiry means wedged rather than slow — a host that is
                // merely slow still returns. Dropping the rest of the batch
                // holds the stall to a single budget instead of one per change;
                // the next change re-triggers the merge from current state,
                // which an idempotent merge handles by construction.
                tracing::warn!(
                    group_id = %self.group_id,
                    "app_data change callback did not return within {:?}; dropping the \
                     remaining {} change(s) in this batch",
                    budget,
                    total - index - 1,
                );
                return;
            }
        }
    }

    /// The group's opaque `app_data` slot as currently committed, read from the
    /// in-memory group state.
    ///
    /// Reads the `APP_DATA` component on its own rather than decoding the whole
    /// mutable-metadata composite. Decoding the composite would let a malformed
    /// *unrelated* component — a corrupt `ADMIN_LIST`, say — fail the read and
    /// make a perfectly good app-data change look unreadable, silently
    /// suppressing the callback the host depends on.
    ///
    /// The outer `Option` separates "could not read the component at all" from
    /// the inner "read fine, no value set" — the caller must not treat an
    /// unreadable group as a cleared slot.
    pub(super) fn read_app_data_slot(mls_group: &OpenMlsGroup) -> Option<Option<String>> {
        use xmtp_mls_common::app_data::components::metadata_attributes::AppDataComponent;

        xmtp_mls_common::app_data::typed_facade::MlsGroupAppData::new(mls_group.extensions())
            .get::<AppDataComponent>()
            .inspect_err(|err| tracing::debug!("could not read the app_data component: {err}"))
            .ok()
    }

    /// A single mutable-metadata attribute as currently committed, read from
    /// the in-memory group state.
    ///
    /// The outer `Option` separates "could not read the metadata at all" from
    /// the inner "read fine, no value set for this field" — callers must not
    /// treat an unreadable group as an unset field.
    pub(super) fn read_metadata_field(
        mls_group: &OpenMlsGroup,
        field_name: &str,
    ) -> Option<Option<String>> {
        // `app_data` has a typed component reader, so use it and keep the guard
        // immune to damage in the rest of the composite. The remaining fields
        // have no string-typed per-component read, so they still go through the
        // full decode.
        if field_name == MetadataField::AppData.as_str() {
            return Self::read_app_data_slot(mls_group);
        }

        let metadata =
            xmtp_mls_common::app_data::component_source::extract_group_mutable_metadata_capability_aware(
                mls_group,
            )
            .inspect_err(|err| {
                tracing::debug!("could not read mutable metadata for field {field_name}: {err}")
            })
            .ok()?;
        Some(metadata.attributes.get(field_name).cloned())
    }

    /// Apply one envelope with state and intent rows from the current writer.
    fn apply_prepared_proposal(
        &self,
        group: &mut OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        envelope: &GroupMessage,
    ) -> Result<Option<ProcessedMessageOutcome>, GroupMessageProcessingError> {
        use xmtp_db::group_intent::QueryPreparedEnvelope;
        if envelope.message.content_type() != openmls::prelude::ContentType::Proposal {
            return Ok(None);
        }
        let db = storage.db();
        for intent in
            // Filter kinds in SQL. An unfiltered load fails outright on a row a
            // newer build wrote with an IntentKind this build cannot decode,
            // and that error is neither retryable nor a safe rejection, so the
            // group head would stop advancing after a downgrade.
            db.find_group_intents(
                self.group_id,
                Some(vec![IntentState::Published]),
                Some(IntentKind::all().collect()),
            )?
        {
            let Some(bytes) = db.prepared_envelopes(intent.id)? else {
                continue;
            };
            let attempt = publish::PreparedAttempt::decode(&bytes)
                .map_err(|error| GroupMessageProcessingError::PreparedAttempt(Box::new(error)))?;
            let Some(proposal) = attempt
                .proposal_for_payload(&envelope.payload_hash)
                .map_err(|error| GroupMessageProcessingError::PreparedAttempt(Box::new(error)))?
            else {
                continue;
            };
            attempt
                .validate_intent(&intent)
                .map_err(|error| GroupMessageProcessingError::PreparedAttempt(Box::new(error)))?;
            let current_epoch = group.epoch().as_u64();
            let message_epoch = envelope.message.epoch().as_u64();
            if message_epoch < current_epoch {
                return Err(GroupMessageProcessingError::OldEpoch(
                    message_epoch,
                    current_epoch,
                ));
            }
            if message_epoch > current_epoch {
                return Err(GroupMessageProcessingError::FutureEpoch(
                    message_epoch,
                    current_epoch,
                ));
            }
            if !attempt.base.matches_epoch(group) {
                return Err(GroupMessageProcessingError::PreparedAttempt(Box::new(
                    publish::OutgoingPreparationError::InvalidPreparedAttempt.into(),
                )));
            }
            self.validate_received_proposal(group, &proposal)?;
            group.store_pending_proposal(storage, proposal)?;
            if attempt.payload_hash == envelope.payload_hash {
                db.set_group_intent_committed(intent.id, envelope.cursor)?;
            }
            return Ok(Some(ProcessedMessageOutcome::new(group.is_active())));
        }
        Ok(None)
    }

    /// Apply one envelope with state and intent rows from the current writer.
    pub(super) fn process_message_inner(
        &self,
        mls_group: &mut OpenMlsGroup,
        storage: &impl XmtpMlsStorageProvider,
        envelope: &GroupMessage,
        event_writer: &impl xmtp_events::EventWriter<crate::subscriptions::internal::InternalEvent>,
    ) -> Result<ProcessedMessageOutcome, GroupMessageProcessingError> {
        let db = storage.db();

        let cursor = db.get_last_cursor(
            self.group_id,
            xmtp_db::refresh_state::EntityKind::ApplicationMessage,
        )?;
        if cursor.0 >= envelope.sequence_id() {
            return Ok(ProcessedMessageOutcome::new(mls_group.is_active()));
        }
        if !mls_group.is_active() {
            return Err(GroupMessageProcessingError::GroupInactive);
        }

        if let Some(outcome) = self.apply_prepared_proposal(mls_group, storage, envelope)? {
            self.maybe_update_cursor(&db, envelope)?;
            return Ok(outcome);
        }

        // An unreadable kind must not reach the external-message path: that
        // would validate a commit we authored against external-actor rules.
        // Hold the head instead of skipping it. This envelope may be a commit
        // every other member applied, so advancing past it would leave this
        // installation behind the group with no way back.
        if db.own_intent_kind_is_unreadable(envelope.payload_hash.as_slice())? {
            return Err(GroupMessageProcessingError::UnsupportedOwnIntentKind(
                hex::encode(&envelope.payload_hash),
            ));
        }
        let intent = db
            .find_group_intent_by_payload_hash(envelope.payload_hash.as_slice())?
            .filter(|intent| intent.group_id == self.group_id);
        let outcome = if let Some(intent) = intent {
            let validated = self
                .stage_and_validate_intent(&db, mls_group, &intent, envelope)
                .map_err(|error| error.processing_error)?;
            let result = self.process_own_message(
                mls_group,
                validated,
                &intent,
                envelope,
                storage,
                event_writer,
            );
            match result {
                Ok(_) => {
                    self.handle_metadata_update_from_intent(&intent, storage)?;
                    db.set_group_intent_committed(intent.id, envelope.cursor)?;
                }
                Err(IntentResolutionError {
                    processing_error: GroupMessageProcessingError::PreCommitProposalPhaseComplete,
                }) => {
                    db.set_group_intent_to_publish(intent.id)?;
                }
                Err(error) => return Err(error.processing_error),
            };
            ProcessedMessageOutcome {
                group_active: mls_group.is_active(),
                app_data_change: None,
            }
        } else {
            self.validate_and_process_external_message(mls_group, envelope, storage, event_writer)?;
            ProcessedMessageOutcome::new(mls_group.is_active())
        };
        // Removal is terminal for unaccepted outgoing work. Abandon it in the
        // same transaction that applies the removal, so no stranded intent can
        // preempt publishing after a later re-add.
        if !outcome.group_active {
            let mut unpublished_messages = Vec::new();
            if !self.conversation_type.is_virtual() {
                for intent in db.find_group_intents(
                    self.group_id,
                    Some(vec![IntentState::ToPublish, IntentState::Published]),
                    Some(IntentKind::all().collect()),
                )? {
                    if let Some(id) = calculate_message_id_for_intent(&intent)?
                        && db.get_group_message(&id)?.is_some_and(|message| {
                            message.delivery_status == DeliveryStatus::Unpublished
                        })
                    {
                        unpublished_messages.push(id);
                    }
                }
            }
            let superseded =
                db.supersede_pending_intents_for_inactive_group(self.group_id.as_ref())?;
            for id in unpublished_messages {
                db.set_delivery_status_to_failed(&id)?;
                self.emit_message_status_changed(
                    id,
                    xmtp_events::MessageStatus::Unpublished,
                    xmtp_events::MessageStatus::Failed,
                    event_writer,
                );
            }
            if superseded > 0 {
                tracing::info!(
                    group_id = %self.group_id,
                    superseded,
                    "superseded pending intents for an inactive group"
                );
            }
        }
        self.maybe_update_cursor(&db, envelope)?;
        Self::save_envelope_metadata_with_db(&db, envelope)?;
        Ok(outcome)
    }

    /// In case of metadataUpdate will extract the updated fields and store them to the db
    fn handle_metadata_update_from_intent(
        &self,
        intent: &StoredGroupIntent,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<(), IntentError> {
        if intent.kind == MetadataUpdate {
            let data = UpdateMetadataIntentData::try_from(intent.data.clone())?;

            match data.field_name.as_str() {
                field_name if field_name == MetadataField::MessageDisappearFromNS.as_str() => {
                    storage.db().update_message_disappearing_from_ns(
                        &self.group_id,
                        data.field_value.parse::<i64>().ok(),
                    )?
                }
                field_name if field_name == MetadataField::MessageDisappearInNS.as_str() => {
                    storage.db().update_message_disappearing_in_ns(
                        &self.group_id,
                        data.field_value.parse::<i64>().ok(),
                    )?
                }
                _ => {} // handle other metadata updates
            }
        }

        Ok(())
    }

    pub(super) fn handle_metadata_update_from_commit(
        &self,
        metadata_field_changes: &Vec<group_updated::MetadataFieldChange>,
        storage: &impl XmtpMlsStorageProvider,
    ) -> Result<(), StorageError> {
        for change in metadata_field_changes {
            match change.field_name.as_str() {
                field_name if field_name == MetadataField::MessageDisappearFromNS.as_str() => {
                    let parsed_value = change
                        .new_value
                        .as_deref()
                        .and_then(|v| v.parse::<i64>().ok());
                    storage
                        .db()
                        .update_message_disappearing_from_ns(&self.group_id, parsed_value)?
                }
                field_name if field_name == MetadataField::MessageDisappearInNS.as_str() => {
                    let parsed_value = change
                        .new_value
                        .as_deref()
                        .and_then(|v| v.parse::<i64>().ok());
                    storage
                        .db()
                        .update_message_disappearing_in_ns(&self.group_id, parsed_value)?
                }
                _ => {} // Handle other metadata updates if needed
            }
        }

        Ok(())
    }
}
