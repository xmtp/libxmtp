//! Prepare and publish durable outgoing attempts.

use super::*;
use crate::groups::app_data::GroupAppDataError;
use xmtp_db::group_intent::QueryPreparedEnvelope;
use xmtp_mls_common::app_data::component_source::ComponentSourceError;
use xmtp_mls_validation::commit::CommitRuleError;

mod dependencies;
mod prepared;
mod rejection;
mod selection;
#[cfg(test)]
mod tests;
mod welcomes;
use dependencies::{PublishDependencies, PublishRequirements};
pub(crate) use prepared::{
    OutgoingPreparationError, PreparedAttempt, PreparedBase, PreparedProposal, Unsettled,
};

/// The status the backend returned for a publish. A failed configuration
/// check before dispatch sent nothing, so its status is not the backend's.
fn backend_status(error: &xmtp_api::ApiError) -> Option<tonic::Code> {
    if xmtp_api::preflight::failure(error).is_some() {
        return None;
    }
    xmtp_proto::api::grpc_status(error).map(|status| status.code())
}

/// Only `INVALID_ARGUMENT` proves that an atomic publish stored nothing.
/// Every other failure, including `OUT_OF_RANGE`, can follow a commit.
pub(crate) fn definite_refusal(error: &xmtp_api::ApiError) -> bool {
    backend_status(error) == Some(tonic::Code::InvalidArgument)
}

/// `OUT_OF_RANGE` can follow a committed publish, and the same request cannot
/// be sent again, so only a recovery read can settle the bytes it carried.
fn out_of_range(error: &xmtp_api::ApiError) -> bool {
    backend_status(error) == Some(tonic::Code::OutOfRange)
}

/// An `AppDataUpdate` that cannot apply to the current component value, such
/// as adding an existing admin or removing a non-admin, fails every attempt.
/// Two admins promoting the same member at once reach this state. So does a
/// commit that would grow the dictionary past its total.
fn inapplicable_app_data(error: &GroupError) -> bool {
    let source = match error {
        GroupError::AppDataCommit(GroupAppDataError::ApplyPayload(source)) => source,
        GroupError::AppDataCommit(GroupAppDataError::Upkeep(inner)) => match inner.as_ref() {
            GroupError::ComponentSource(source) => source,
            // implements: META-077
            GroupError::CommitValidation(CommitValidationError::Rule(
                CommitRuleError::InsufficientPermissions,
            )) => return true,
            _ => return false,
        },
        _ => return false,
    };
    matches!(
        source,
        ComponentSourceError::TlsSetApply(_) | ComponentSourceError::TlsMapApply(_)
    )
}

/// The next durable attempt, or immutable inputs that need external resolution.
enum NextPublish {
    Prepared(StoredGroupIntent, PreparedAttempt),
    Resolve(PublishRequirements),
    /// A message behind a deferred message. It stays as it is.
    Behind(ID),
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Persist each attempt with its crypto state before publication. An ambiguous
    /// result retries the saved bytes, except `OUT_OF_RANGE`, which a recovery
    /// read settles instead. A state-changing attempt blocks later work until
    /// ordered processing resolves it.
    #[xmtp_common::mls_span(redact_error)]
    pub(in crate::groups) async fn publish_intents(&self) -> Result<(), GroupError> {
        // Nothing this client prepared is published once
        // its connection is blocked. The intent stays queued for a client that can.
        self.context.server_configuration().check()?;
        let mut sent = HashSet::new();
        let mut after = None;
        let mut rejected_request = None;
        // A message waits behind a pending proposal. Every later message
        // waits behind it, and this pass commits the proposal at most once.
        let mut deferred_message = false;
        let mut queued_commit = false;
        let upper_id = self
            .context
            .db()
            .last_publish_intent_id(self.group_id.as_ref())?;
        let Some(mut upper_id) = upper_id else {
            return Ok(());
        };

        loop {
            let next = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
                tx.with_group(self.group_id, |group, storage| {
                    let intent = storage.db().next_publish_intent(
                        self.group_id.as_ref(),
                        after,
                        upper_id,
                    )?;
                    let Some(intent) = intent else {
                        return Ok(Continue(None));
                    };
                    if sent.contains(&intent.id) {
                        return Ok(Continue(None));
                    }
                    // implements: SEND-022
                    // Once a message is deferred, no later message publishes
                    // in this pass, whether or not it holds a saved attempt.
                    // Another member's commit can clear the proposal meanwhile.
                    if deferred_message && intent.kind == IntentKind::SendMessage {
                        return Ok(Continue(Some(NextPublish::Behind(intent.id))));
                    }
                    if intent.state == IntentState::Published {
                        let bytes = storage
                            .db()
                            .prepared_envelopes(intent.id)?
                            .ok_or(OutgoingPreparationError::MissingPreparedAttempt(intent.id))?;
                        let attempt = PreparedAttempt::decode(&bytes)?;
                        attempt.validate_intent(&intent)?;
                        return Ok(Continue(Some(NextPublish::Prepared(intent, attempt))));
                    }
                    if storage.db().prepared_envelopes(intent.id)?.is_some() {
                        return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                    }
                    let requirements = PublishRequirements::capture(group, &intent)?;
                    Ok::<_, GroupError>(Continue(Some(NextPublish::Resolve(requirements))))
                })
            })?
            .into_continued();
            let Some(next) = next else {
                // implements: SEND-022
                // No later intent committed the proposal. Commit it now; the
                // deferred messages publish in the next epoch.
                if deferred_message && !queued_commit {
                    queued_commit = true;
                    upper_id = QueueIntent::commit_pending_proposals().queue(self)?.id;
                    continue;
                }
                return rejected_request.map_or(Ok(()), Err);
            };
            let (intent, attempt) = match next {
                NextPublish::Prepared(intent, attempt) => (intent, attempt),
                NextPublish::Behind(id) => {
                    after = Some(id);
                    continue;
                }
                NextPublish::Resolve(mut requirements) => {
                    let result = loop {
                        let mut dependencies =
                            match self.resolve_publish_dependencies(&requirements).await {
                                Ok(dependencies) => dependencies,
                                Err(error) => break Err(error),
                            };
                        let result = self.prepare_publish_attempt(&requirements, &mut dependencies);
                        if matches!(
                            result,
                            Err(GroupError::WrappedApi(xmtp_api::ApiError::EnvelopeTooLarge))
                        ) {
                            match self.smaller_membership_refresh(&requirements) {
                                Ok(Some(smaller)) => {
                                    requirements = smaller;
                                    continue;
                                }
                                Ok(None) => {}
                                Err(error) => break Err(error),
                            }
                        }
                        break result.map(|attempt| {
                            attempt.map(|attempt| {
                                (
                                    dependencies
                                        .prepared_intent
                                        .unwrap_or_else(|| requirements.intent.clone()),
                                    attempt,
                                )
                            })
                        });
                    };
                    match result {
                        Err(GroupError::OutgoingPreparation(
                            OutgoingPreparationError::StateChanged,
                        )) => {
                            continue;
                        }
                        // implements: SEND-022
                        // No message can be built while a proposal is pending.
                        // Leave it queued and prepare the later state changes:
                        // one of them can commit the proposal. Each skip
                        // advances past the intent, so this loop still ends.
                        Err(GroupError::CreateMessage(
                            openmls::prelude::CreateMessageError::GroupStateError(
                                openmls::prelude::MlsGroupStateError::PendingProposal,
                            ),
                        )) if requirements.intent.kind == IntentKind::SendMessage => {
                            deferred_message = true;
                            after = Some(requirements.intent.id);
                            #[cfg(test)]
                            tests::after_deferral(self.group_id.as_ref()).await;
                            continue;
                        }
                        Err(error)
                            if matches!(
                                error,
                                GroupError::WrappedApi(
                                    xmtp_api::ApiError::EnvelopeTooLarge
                                        | xmtp_api::ApiError::UnitTooLarge
                                        | xmtp_api::ApiError::InvalidRequest(_)
                                        // The local admission parse refuses
                                        // what the backend would refuse.
                                        | xmtp_api::ApiError::InvalidEnvelope(_)
                                ) | GroupError::OutgoingPreparation(
                                    // A group without a root cannot sign, and
                                    // a failed self-check releases nothing.
                                    OutgoingPreparationError::GroupKeys(_)
                                        | OutgoingPreparationError::SignGroupMessage(_)
                                ) | GroupError::CommitValidation(CommitValidationError::Rule(
                                    CommitRuleError::InsufficientPermissions
                                )) | GroupError::InvalidGroupMembership
                                    | GroupError::InvalidPublicKeys(_)
                                    | GroupError::MetadataField(_)
                                    // A malformed inbox id fails every attempt.
                                    // Earlier builds queued admin-list intents
                                    // without parsing the inbox id.
                                    | GroupError::ComponentSource(
                                        ComponentSourceError::InvalidInboxId(_)
                                    )
                                    // Earlier builds queued reserved transcript sends.
                                    | GroupError::ReservedTranscriptContentType
                            ) || inapplicable_app_data(&error) =>
                        {
                            if self.reject_unprepared_request(&requirements)? {
                                sent.insert(requirements.intent.id);
                                after = Some(requirements.intent.id);
                                rejected_request.get_or_insert(error);
                            }
                            continue;
                        }
                        Err(error) => return Err(error),
                        Ok(None) => {
                            sent.insert(requirements.intent.id);
                            after = Some(requirements.intent.id);
                            continue;
                        }
                        Ok(Some(prepared)) => prepared,
                    }
                }
            };
            sent.insert(intent.id);
            // A blocker can be ahead of earlier messages. Do not skip those messages.
            if intent.state != IntentState::Published || intent.kind == IntentKind::SendMessage {
                after = Some(intent.id);
            }
            if attempt.receipts.is_none() {
                // The writer and every mutable MLS object were dropped above.
                // An ambiguous error keeps this exact attempt eligible for retry.
                //
                // Saved bytes that no longer fit in one request cannot be sent
                // again: an earlier build saved more envelopes than the
                // request bound allows, or the deployment lowered a limit.
                // They may already be stored, so a recovery read settles them,
                // as after `OUT_OF_RANGE`.
                let (unit, unsendable) = match attempt.publish_unit(self.context.api().limits()) {
                    Ok(unit) => (unit, false),
                    Err(GroupError::WrappedApi(
                        xmtp_api::ApiError::UnitTooLarge | xmtp_api::ApiError::EnvelopeTooLarge,
                    )) => (attempt.settlement_unit()?, true),
                    Err(error) => return Err(error),
                };
                let stored = if unsendable || attempt.unsettled == Some(Unsettled::Attempt) {
                    self.settle_attempt(&intent, &attempt, unit).await?
                } else {
                    match self
                        .context
                        .api()
                        .send_group_messages(vec![unit.clone()])
                        .await
                    {
                        Ok(receipts) => {
                            self.record_publish_receipts(&intent, &attempt, receipts)?;
                            true
                        }
                        Err(error) if definite_refusal(&error) => {
                            if self.reject_refused_attempt(&intent, &attempt)? {
                                rejected_request.get_or_insert(error.into());
                            }
                            continue;
                        }
                        Err(error) if out_of_range(&error) => {
                            self.mark_unsettled(intent.id, &attempt, Unsettled::Attempt)?;
                            self.settle_attempt(&intent, &attempt, unit).await?
                        }
                        Err(error) => return Err(error.into()),
                    }
                };
                if !stored {
                    if self.reject_refused_attempt(&intent, &attempt)? {
                        rejected_request.get_or_insert(OutgoingPreparationError::Unstored.into());
                    }
                    continue;
                }
            }
            if intent.kind != IntentKind::SendMessage {
                return rejected_request.map_or(Ok(()), Err);
            }
        }
    }

    /// Settle an attempt whose publish returned `OUT_OF_RANGE` with a `Query`
    /// of its topic instead of the request, which cannot be sent again. Stored
    /// envelopes get the receipts the publish assigned. Returns false when the
    /// backend stored none of them, which proves that no echo can arrive.
    // implements: SEND-007
    async fn settle_attempt(
        &self,
        intent: &StoredGroupIntent,
        attempt: &PreparedAttempt,
        unit: xmtp_api::PublishUnit,
    ) -> Result<bool, GroupError> {
        let settled = self.context.api().settle_units(&[unit]).await?;
        if settled.iter().all(Option::is_none) {
            return Ok(false);
        }
        let receipts = settled.into_iter().collect::<Option<Vec<_>>>().ok_or(
            xmtp_api::ApiError::InvalidResponse("partially stored atomic publish"),
        )?;
        self.record_publish_receipts(intent, attempt, receipts)?;
        Ok(true)
    }

    /// Fail a locally invalid request only if its intent and MLS base still match.
    /// The failed preparation has already rolled back all crypto changes.
    fn reject_unprepared_request(
        &self,
        requirements: &PublishRequirements,
    ) -> Result<bool, GroupError> {
        // The failed preparation transaction already rolled back its ratchets.
        // Only definite local request or authorization errors enter this path.
        crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, event_writer| {
                tx.with_group(self.group_id, |group, storage| {
                    let db = storage.db();
                    let Some(current) =
                        Fetch::<StoredGroupIntent>::fetch(&db, &requirements.intent.id)?
                    else {
                        return Ok(Continue(false));
                    };
                    if current.state != IntentState::ToPublish
                        || current.data != requirements.intent.data
                        || current.kind != requirements.intent.kind
                        || PreparedBase::capture(group)? != requirements.base
                        || db.prepared_envelopes(current.id)?.is_some()
                    {
                        return Ok(Continue(false));
                    }
                    self.fail_intent(&db, &current, event_writer)?;
                    Ok::<_, GroupError>(Continue(true))
                })
            },
        )
        .map(TransactionOutcome::into_continued)
    }

    /// Fail an attempt the backend refused, only while it is still the current
    /// attempt with no receipt. A late refusal of replaced bytes changes nothing.
    // implements: SEND-009
    fn reject_refused_attempt(
        &self,
        intent: &StoredGroupIntent,
        attempt: &PreparedAttempt,
    ) -> Result<bool, GroupError> {
        crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, event_writer| {
                let storage = tx.storage();
                let db = storage.db();
                let Some(current) = Fetch::<StoredGroupIntent>::fetch(&db, &intent.id)? else {
                    return Ok(Continue(false));
                };
                let Some(saved) = db.prepared_envelopes(current.id)? else {
                    return Ok(Continue(false));
                };
                let saved = PreparedAttempt::decode(&saved)?;
                if current.state != IntentState::Published
                    || !saved.same_attempt(attempt)
                    || saved.receipts.is_some()
                {
                    return Ok(Continue(false));
                }
                self.fail_intent(&db, &current, event_writer)?;
                Self::fail_membership_parent(&db, &current, &saved)?;
                Ok::<_, GroupError>(Continue(true))
            },
        )
        .map(TransactionOutcome::into_continued)
    }

    /// A terminal failure of a split part also fails its requested target.
    /// Called under the same writer that records the child's failure.
    fn fail_membership_parent(
        db: &impl DbQuery,
        child: &StoredGroupIntent,
        attempt: &PreparedAttempt,
    ) -> Result<(), GroupError> {
        if let Some(parent_id) = attempt.parent_intent
            && let Some(parent) = Fetch::<StoredGroupIntent>::fetch(db, &parent_id)?
            && parent.group_id == child.group_id
            && parent.kind == IntentKind::UpdateGroupMembership
            && parent.state == IntentState::ToPublish
        {
            db.set_group_intent_error(parent.id)?;
        }
        Ok(())
    }

    /// Mark the intent `Error` and its application message `Failed` under the
    /// caller's writer, so that no later intent waits for an echo.
    fn fail_intent(
        &self,
        db: &impl DbQuery,
        intent: &StoredGroupIntent,
        event_writer: &impl xmtp_events::EventWriter<crate::subscriptions::internal::InternalEvent>,
    ) -> Result<(), GroupError> {
        let message_id = calculate_message_id_for_intent(intent)?;
        let previous_status = message_id
            .as_ref()
            .map(|id| db.get_group_message(id))
            .transpose()?
            .flatten()
            .map(|message| message.delivery_status);
        db.set_group_intent_error_and_fail_msg(intent, message_id.clone())?;
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
        Ok(())
    }

    /// Reload and check the base under one writer, then atomically save crypto
    /// state, exact envelopes, proposal records, and the published intent.
    fn prepare_publish_attempt(
        &self,
        requirements: &PublishRequirements,
        dependencies: &mut PublishDependencies,
    ) -> Result<Option<PreparedAttempt>, GroupError> {
        let (storage, bus) = (self.context.mls_storage(), self.context.events());
        crate::state_tx::state_write_with_events_durable(storage, bus, |tx, events| {
            let mut no_op = None;
            let result = tx.savepoint(|tx| {
                tx.with_group(self.group_id, |group, storage| {
                    let db = storage.db();
                    let Some(mut intent) =
                        Fetch::<StoredGroupIntent>::fetch(&db, &requirements.intent.id)?
                    else {
                        return Ok(Continue(None));
                    };
                    if intent.state != IntentState::ToPublish {
                        return Err(OutgoingPreparationError::StateChanged.into());
                    }
                    if intent.data != requirements.intent.data
                        || intent.kind != requirements.intent.kind
                        || intent.should_push != requirements.intent.should_push
                        || PreparedBase::capture(group)? != requirements.base
                    {
                        return Err(OutgoingPreparationError::StateChanged.into());
                    }
                    if db.prepared_envelopes(intent.id)?.is_some() {
                        return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                    }
                    if group.pending_commit().is_some() {
                        return Err(OutgoingPreparationError::UnexpectedPendingCommit.into());
                    }
                    let has_pending_change =
                        db.has_published_group_change(self.group_id.as_ref())?;
                    if has_pending_change {
                        return Err(OutgoingPreparationError::StateChanged.into());
                    }
                    dependencies.validate_local(storage)?;
                    if let Some(data) = &requirements.membership_update {
                        intent.data = Vec::from(data.clone()).into();
                    }
                    let original_proposals = group.pending_proposals().cloned().collect::<Vec<_>>();
                    let replacement_payloads = dependencies.selection.apply(
                        group,
                        storage,
                        &self.context.identity().installation_keys,
                    )?;
                    // Receivers still judge the rest on read-back, so no
                    // pre-publish check runs for an omission alone.
                    dependencies.omitted_uncommittable = selection::commits_proposals(intent.kind)
                        && selection::omit_uncommittable_proposals(group, storage)?;
                    let Some(mut data) =
                        self.get_publish_intent_data(storage, group, &intent, dependencies)?
                    else {
                        let state = Fetch::<StoredGroupIntent>::fetch(&db, &intent.id)?
                            .map_or(IntentState::ToPublish, |current| current.state);
                        // A no-op may follow replacement signing. Roll back the
                        // whole trial, including sender ratchets and generated keys.
                        no_op = Some(state);
                        return Ok(Rollback);
                    };
                    if dependencies.selection.active
                        && let Some(staged) = &data.staged_commit
                    {
                        // implements: GMOD-038
                        // Validate the selected set before publication.
                        // Ordered read-back validation still applies to every commit.
                        ValidatedCommit::from_staged_commit_local(
                            &self.context,
                            &db,
                            &decode_staged_commit(staged)?,
                            group.own_leaf_index(),
                            group,
                            u64::MAX,
                        )?;
                    }
                    data.payloads_to_publish.splice(0..0, replacement_payloads);
                    // Read the persisted proposal order under the same writer before
                    // recording the exact outgoing batch.
                    group.reload(storage)?;
                    let new_proposals = group
                        .pending_proposals()
                        .filter(|proposal| {
                            !original_proposals.iter().any(|original| {
                                original.proposal_reference_ref()
                                    == proposal.proposal_reference_ref()
                            })
                        })
                        .cloned()
                        .collect::<Vec<_>>();
                    let non_proposal_payloads = usize::from(data.staged_commit.is_some())
                        + usize::from(intent.kind == IntentKind::SendMessage);
                    if new_proposals.len() + non_proposal_payloads != data.payloads_to_publish.len()
                    {
                        return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                    }
                    // The pinned OpenMLS ProposalStore is insertion-ordered. The SQL
                    // proposal-reference list keeps the same order across reloads.
                    // Each sender emits standalone proposals first, in that order.
                    let proposals = new_proposals
                        .iter()
                        .zip(&data.payloads_to_publish)
                        .map(|(proposal, payload)| {
                            Ok(PreparedProposal {
                                payload_hash: sha256(payload).to_vec(),
                                proposal: xmtp_db::db_serialize(proposal)?,
                            })
                        })
                        .collect::<Result<Vec<_>, GroupError>>()?;
                    selection::replace_proposals(group, storage, &original_proposals)?;
                    let payload = data
                        .payloads_to_publish
                        .last()
                        .ok_or(GroupError::UninitializedResult)?;
                    // Sign after each sender HMAC and before the attempt is
                    // saved, so every saved envelope carries its signature.
                    let envelopes = self.prepare_group_envelopes_in(
                        &db,
                        events,
                        group,
                        data.payloads_to_publish
                            .iter()
                            .map(|payload| (payload.as_slice(), data.should_send_push_notification))
                            .collect(),
                    )?;
                    let attempt = PreparedAttempt {
                        version: prepared::PREPARED_ATTEMPT_VERSION,
                        base: requirements.base.clone(),
                        parent_intent: requirements
                            .partial_membership_update
                            .then_some(requirements.intent.id),
                        payload_hash: sha256(payload).to_vec(),
                        envelopes: envelopes
                            .iter()
                            .map(prost::Message::encode_to_vec)
                            .collect(),
                        proposals,
                        receipts: None,
                        welcomes: None,
                        rejection: None,
                        unsettled: None,
                    };
                    // Validate size and shape before any ratchet writes can commit.
                    attempt.publish_unit(self.context.api().limits())?;
                    if requirements.partial_membership_update {
                        // Leave the original desired target pending. Its next
                        // diff against the advanced group contains only the
                        // unfinished inboxes. The published child preempts it.
                        intent = QueueIntent::update_group_membership()
                            .data(intent.data.as_slice().to_vec())
                            .should_push(intent.should_push)
                            .queue_in(&db, self)?;
                        dependencies.prepared_intent = Some(intent.clone());
                    }
                    let encoded = xmtp_db::db_serialize(&attempt)?;
                    if !db.compare_and_set_prepared_envelopes(intent.id, None, Some(&encoded))? {
                        return Err(OutgoingPreparationError::StateChanged.into());
                    }
                    db.set_group_intent_published(
                        intent.id,
                        &attempt.payload_hash,
                        data.post_commit_action,
                        data.staged_commit,
                        data.group_epoch as i64,
                    )?;
                    Ok::<_, GroupError>(Continue(Some(attempt)))
                })
            })?;
            match result {
                Continue(attempt) => Ok(Continue(attempt)),
                Rollback => {
                    let state = no_op.ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
                    let storage = tx.storage();
                    let db = storage.db();
                    if state == IntentState::Superseded {
                        db.set_group_intent_superseded(requirements.intent.id)?;
                    } else if state == IntentState::ToPublish {
                        db.set_group_intent_processed(requirements.intent.id)?;
                    }
                    Ok(Continue(None))
                }
            }
        })
        .map(TransactionOutcome::into_continued)
    }

    /// Create one attempt under the caller's writer. This performs no network I/O.
    /// `None` means that the intent makes no change to the current group.
    #[allow(clippy::type_complexity)]
    #[tracing::instrument(level = "trace", skip_all)]
    // implements: META-065
    fn get_publish_intent_data(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        openmls_group: &mut OpenMlsGroup,
        intent: &StoredGroupIntent,
        dependencies: &mut PublishDependencies,
    ) -> Result<Option<PublishIntentData>, GroupError> {
        let provider = XmtpOpenMlsProviderRef::new(storage);
        match intent.kind {
            IntentKind::UpdateGroupMembership => {
                let intent_data =
                    UpdateGroupMembershipIntentData::try_from(intent.data.as_slice())?;
                let signer = &self.context.identity().installation_keys;
                apply_update_group_membership_intent(
                    storage,
                    openmls_group,
                    intent_data,
                    dependencies.take_changes()?,
                    &self
                        .context
                        .server_configuration()
                        .configuration()
                        .application_components,
                    signer,
                )
            }
            IntentKind::SendMessage => {
                // We can safely assume all SendMessage intents have data
                let intent_data = SendMessageIntentData::from_bytes(intent.data.as_slice())?;
                // An older build queued this intent before the creation check.
                // implements: GMOD-054
                // The envelope content can hold a device-sync HMAC root key;
                // the decoded envelope wipes it on drop.
                if let Ok(PlaintextEnvelope {
                    content: Some(Content::V1(v1)),
                }) = PlaintextEnvelope::decode(intent_data.message.as_slice())
                    && Self::is_reserved_transcript_content(&v1.content)
                {
                    return Err(GroupError::ReservedTranscriptContentType);
                }
                // Pending proposals are handled at the API level (in send_message)
                // by committing them before creating the SendMessage intent
                let group_epoch = openmls_group.epoch().as_u64();
                let msg = openmls_group.create_message(
                    &provider,
                    &self.context.identity().installation_keys,
                    intent_data.message.as_slice(),
                )?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![msg.tls_serialize_detached()?],
                    post_commit_action: None,
                    staged_commit: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::KeyUpdate => {
                let keys = self.context.identity().installation_keys.clone();
                let (bundle, staged_commit, group_epoch) =
                    generate_prepared_commit(storage, openmls_group, |group, provider| {
                        let (upkeep, updates) =
                            crate::groups::app_data::membership_upkeep::membership_upkeep(
                                group,
                                &self
                                    .context
                                    .server_configuration()
                                    .configuration()
                                    .application_components,
                            )?;
                        let mut stage = crate::groups::mls_ext::commit_without_group_info(
                            group,
                            provider.storage(),
                            |builder| {
                                builder
                                    .leaf_node_parameters(LeafNodeParameters::default())
                                    .consume_proposal_store(true)
                                    .add_proposals(upkeep)
                            },
                        )
                        .map_err(CommitToPendingProposalsError::from)?;
                        stage.with_app_data_dictionary_updates(updates);
                        let bundle = stage
                            .build(provider.rand(), provider.crypto(), &keys, |_| true)
                            .map_err(CommitToPendingProposalsError::from)?
                            .stage_commit(provider)
                            .map_err(CommitToPendingProposalsError::from)?;
                        Ok::<_, GroupError>(bundle)
                    })?;
                let (commit, welcome, _group_info) = bundle.into_messages();
                let post_commit_action = update_group_membership::welcome_post_commit_action(
                    welcome,
                    staged_commit.as_deref(),
                )?;
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit.tls_serialize_detached()?],
                    staged_commit,
                    post_commit_action,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::MetadataUpdate => {
                let metadata_intent =
                    UpdateMetadataIntentData::try_from(intent.data.as_slice().to_vec())?;

                // Compare-and-swap guard. This runs on every publish attempt,
                // including the republish after an intent loses an epoch race,
                // which is exactly when the frozen `field_value` has gone
                // stale. Abandoning here is what stops the intent from
                // silently overwriting whatever landed in the meantime.
                //
                // Marked `Superseded` (not `Error`) and reported as
                // `Ok(None)`, so it is terminal without burning publish
                // attempts or aborting the publish loop for the other intents
                // queued on this group.
                if let Some(expected) = &metadata_intent.expected_field_value {
                    // A failed read is not evidence the guard was violated —
                    // leave the intent alone and let the normal error path
                    // surface whatever is actually wrong. The outer `Option`
                    // separates "unreadable" from "readable but unset".
                    if let Some(committed) =
                        Self::read_metadata_field(openmls_group, &metadata_intent.field_name)
                        && committed.as_deref() != Some(expected.as_str())
                    {
                        tracing::info!(
                            group_id = %self.group_id,
                            intent_id = intent.id,
                            field = %metadata_intent.field_name,
                            "abandoning guarded metadata update: committed value no longer matches"
                        );
                        storage.db().set_group_intent_superseded(intent.id)?;
                        return Ok(None);
                    }
                }

                // Publish a standalone AppDataUpdate proposal followed by a
                // commit that references it. Both wire messages go in one
                // publish batch, with the proposal first.
                use crate::groups::app_data::stage_app_data_propose_and_commit;
                use xmtp_mls_common::app_data::component_source::metadata_field_to_component_id;

                let component_id = metadata_field_to_component_id(&metadata_intent.field_name)
                    .ok_or_else(|| {
                        GroupError::ComponentSource(ComponentSourceError::UnknownMetadataField(
                            metadata_intent.field_name.clone(),
                        ))
                    })?;

                let payload = xmtp_mls_common::app_data::creation::encode_metadata_attribute_value(
                    component_id,
                    &metadata_intent.field_value,
                )
                .map_err(crate::groups::app_data::migration::BootstrapSynthesisError::from)?;

                let signer = self.context.identity().installation_keys.clone();
                let catalogue = &self
                    .context
                    .server_configuration()
                    .configuration()
                    .application_components;
                let ((proposal_msg, bundle), staged_commit, group_epoch) =
                    generate_prepared_commit(
                        storage,
                        openmls_group,
                        // `payload` can be the commit-log signer. It stays in
                        // `Zeroizing` until it moves into OpenMLS, so a
                        // return before this closure runs still wipes it.
                        move |group, provider| -> Result<_, GroupError> {
                            let mut payload = payload;
                            Ok(stage_app_data_propose_and_commit(
                                group,
                                provider,
                                &signer,
                                catalogue,
                                component_id,
                                std::mem::take(&mut *payload),
                            )?)
                        },
                    )?;

                let (commit, welcome, _group_info) = bundle.into_messages();
                let post_commit_action = update_group_membership::welcome_post_commit_action(
                    welcome,
                    staged_commit.as_deref(),
                )?;
                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![
                        proposal_msg.tls_serialize_detached()?,
                        commit.tls_serialize_detached()?,
                    ],
                    staged_commit,
                    post_commit_action,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::UpdateAdminList => {
                let admin_list_update_intent =
                    UpdateAdminListIntentData::try_from(intent.data.as_slice().to_vec())?;

                let signer = self.context.identity().installation_keys.clone();
                let publish =
                    crate::groups::app_data::sender_intents::apply_update_admin_list_app_data_intent(
                        storage,
                        openmls_group,
                        admin_list_update_intent,
                        &self
                            .context
                            .server_configuration()
                            .configuration()
                            .application_components,
                        signer,
                        intent.should_push,
                    )?;
                Ok(Some(publish))
            }
            IntentKind::UpdatePermission => {
                let update_permissions_intent =
                    UpdatePermissionIntentData::try_from(intent.data.as_slice().to_vec())?;

                let signer = self.context.identity().installation_keys.clone();
                let publish =
                    crate::groups::app_data::sender_intents::apply_update_permission_app_data_intent(
                        storage,
                        openmls_group,
                        update_permissions_intent,
                        &self
                            .context
                            .server_configuration()
                            .configuration()
                            .application_components,
                        signer,
                        intent.should_push,
                    )?;
                Ok(Some(publish))
            }
            IntentKind::ReaddInstallations => {
                let intent_data = ReaddInstallationsIntentData::try_from(intent.data.as_slice())?;
                let signer = &self.context.identity().installation_keys;
                apply_readd_installations_intent(
                    storage,
                    openmls_group,
                    intent_data,
                    dependencies.take_changes()?,
                    &self
                        .context
                        .server_configuration()
                        .configuration()
                        .application_components,
                    signer,
                )
            }
            IntentKind::ProposeMemberUpdate => {
                let intent_data = ProposeMemberUpdateIntentData::try_from(intent.data.as_slice())?;
                update_group_membership::reject_super_admin_removal(
                    openmls_group,
                    &intent_data.remove_inbox_ids,
                )?;
                let group_epoch = openmls_group.epoch().as_u64();
                let signer = &self.context.identity().installation_keys;
                let mut proposal_payloads = Vec::new();

                // The membership the AppDataUpdate proposal will encode on
                // migrated groups. We mutate this as we process adds/removes
                // so it ends up reflecting only the inbox_ids that actually
                // got Add proposals (i.e. had at least one key package that
                // fetched successfully) plus any explicit removes — not the
                // raw intent.
                let extensions: Extensions<GroupContext> = openmls_group.extensions().clone();
                let old_group_membership = extract_group_membership(&extensions)?;
                let mut new_membership = old_group_membership.clone();

                // Handle adds
                if !intent_data.add_inbox_ids.is_empty() {
                    let latest_sequence_ids = &dependencies.latest_sequence_ids;
                    let changes_with_kps = dependencies
                        .changes
                        .as_ref()
                        .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;

                    // If we failed to fetch key packages for all installations, error
                    if !changes_with_kps.failed_installations.is_empty()
                        && changes_with_kps.new_key_packages.is_empty()
                    {
                        return Err(GroupError::FailedToVerifyInstallations(
                            FailedInstallationIds(changes_with_kps.failed_installations.clone()),
                        ));
                    }

                    // Compute the inbox_ids that actually got at least one
                    // key package — those are the only ones that should
                    // appear in the AppDataUpdate payload below. An
                    // inbox_id whose installations all failed kp fetch has
                    // no MLS leaf in the commit, so claiming membership
                    // for it would diverge dict and tree state.
                    let added_inbox_ids = update_group_membership::inbox_ids_from_new_key_packages(
                        &changes_with_kps.new_key_packages,
                    );
                    for inbox_id in &intent_data.add_inbox_ids {
                        if !added_inbox_ids.contains(inbox_id) {
                            continue;
                        }
                        let sequence_id = latest_sequence_ids
                            .get(inbox_id.as_str())
                            .copied()
                            .ok_or(GroupError::MissingSequenceId)?;
                        new_membership.add(inbox_id.clone(), sequence_id as u64);
                    }

                    // Keep the failed-installations set on the local
                    // membership so the equality check detects key-package
                    // failure-only deltas.
                    new_membership.failed_installations =
                        changes_with_kps.failed_installations.clone();

                    // Generate add proposals for each key package
                    for key_package in &changes_with_kps.new_key_packages {
                        if openmls_group.pending_proposals().any(|proposal| {
                            matches!(proposal.proposal(), Proposal::Add(add) if add.key_package() == key_package)
                        }) {
                            continue;
                        }
                        let (proposal_msg, _proposal_ref) = openmls_group
                            .propose_add_member(&provider, signer, key_package)
                            .map_err(GroupError::ProposeAddMember)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }
                }

                // Handle removes
                if !intent_data.remove_inbox_ids.is_empty() {
                    let inbox_ids_to_remove: HashSet<_> =
                        intent_data.remove_inbox_ids.iter().cloned().collect();
                    let mut members_to_remove = Vec::new();
                    for member in openmls_group.members() {
                        let credential = BasicCredential::try_from(member.credential.clone())?;
                        let member_inbox_id = parse_credential(credential.identity())?;
                        if inbox_ids_to_remove.contains(&member_inbox_id) {
                            members_to_remove.push(member.index);
                        }
                    }

                    // Generate remove proposals for collected members
                    for member_index in members_to_remove {
                        if openmls_group.pending_proposals().any(|proposal| {
                            matches!(proposal.proposal(), Proposal::Remove(remove) if remove.removed() == member_index)
                        }) {
                            continue;
                        }
                        let (proposal_msg, _proposal_ref) = openmls_group
                            .propose_remove_member(&provider, signer, member_index)
                            .map_err(GroupError::ProposeRemoveMember)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }

                    for inbox_id in &intent_data.remove_inbox_ids {
                        new_membership.remove(inbox_id);
                    }
                }

                if old_group_membership != new_membership {
                    use crate::groups::mls_sync::update_group_membership::build_group_membership_app_data_payload;

                    let payload = build_group_membership_app_data_payload(
                        &storage.db(),
                        openmls_group,
                        &old_group_membership,
                        &new_membership,
                    )?;
                    if !update_group_membership::has_pending_membership_delta(
                        openmls_group,
                        &payload,
                    )? {
                        let (proposal_msg, _) = openmls_group
                            .propose_app_data_update(
                                &provider,
                                signer,
                                xmtp_mls_common::app_data::component_id::ComponentId::GROUP_MEMBERSHIP
                                    .as_u16(),
                                openmls::messages::proposals::AppDataUpdateOperation::Update(
                                    payload.into(),
                                ),
                            )
                            .map_err(GroupError::Proposal)?;
                        proposal_payloads.push(proposal_msg.tls_serialize_detached()?);
                    }
                }

                // A repeated request can already be represented by accepted
                // proposals. Keep their original references and proposers.
                if proposal_payloads.is_empty() {
                    return Ok(None);
                }

                Ok(Some(PublishIntentData {
                    payloads_to_publish: proposal_payloads,
                    staged_commit: None,
                    post_commit_action: None,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
            IntentKind::ProposeGroupContextExtensions => Err(CommitValidationError::Rule(
                CommitRuleError::UnsupportedProposalType(ProposalType::GroupContextExtensions),
            )
            .into()),
            IntentKind::BootstrapMigration => Err(CommitValidationError::Rule(
                CommitRuleError::UnsupportedProposalType(ProposalType::GroupContextExtensions),
            )
            .into()),
            IntentKind::AppDataUpdate => {
                let intent_data = crate::groups::intents::AppDataUpdateIntentData::try_from(
                    intent.data.as_slice(),
                )?;
                let own = xmtp_mls_common::inbox_id::InboxId::from_hex(self.context.inbox_id())
                    .map_err(|e| GroupError::ComponentSource(e.into()))?;
                crate::groups::app_data::sender_intents::apply_app_data_update_intent(
                    storage,
                    openmls_group,
                    intent_data,
                    own,
                    &self
                        .context
                        .server_configuration()
                        .configuration()
                        .application_components,
                    self.context.identity().installation_keys.clone(),
                    intent.should_push,
                )
            }
            IntentKind::CommitPendingProposals => {
                let _intent_data =
                    CommitPendingProposalsIntentData::try_from(intent.data.as_slice())?;

                // Check if there are any pending proposals to commit
                // An omitted proposal expires with the epoch, so a commit
                // that carries no proposal is still valid work.
                if openmls_group.pending_proposals().next().is_none()
                    && !dependencies.selection.active
                    && !dependencies.omitted_uncommittable
                {
                    tracing::debug!("No pending proposals to commit");
                    return Ok(None);
                }

                let signer = &self.context.identity().installation_keys;

                let (bundle, staged_commit, group_epoch) =
                    generate_prepared_commit(storage, openmls_group, |group, provider| {
                        build_commit_with_pending_app_data_updates(
                            group,
                            provider,
                            signer,
                            &self
                                .context
                                .server_configuration()
                                .configuration()
                                .application_components,
                            |_| true,
                        )
                    })?;
                let (commit, maybe_welcome, _group_info) = bundle.into_messages();
                let staged_commit =
                    staged_commit.ok_or_else(|| GroupError::MissingPendingCommit)?;
                let post_commit_action = update_group_membership::welcome_post_commit_action(
                    maybe_welcome,
                    Some(&staged_commit),
                )?;

                Ok(Some(PublishIntentData {
                    payloads_to_publish: vec![commit.tls_serialize_detached()?],
                    staged_commit: Some(staged_commit),
                    post_commit_action,
                    should_send_push_notification: intent.should_push,
                    group_epoch,
                }))
            }
        }
    }
}
