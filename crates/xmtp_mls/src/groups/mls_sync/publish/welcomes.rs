//! Required Welcome publication after an ordered commit.

use super::*;
use prost::Message;
use serde::{Deserialize, Serialize};
use xmtp_proto::backend_v1::EnvelopeMeta;
use xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperAlgorithm;

// Byte fields are CBOR byte strings, as in `prepared.rs`. rc4 is the first
// release that stores them as CBOR, so no reader of the integer-array form
// exists.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Exact follow-up bytes anchored to one accepted membership commit.
pub(crate) struct PreparedWelcomes {
    /// Ordered commit cursor included in the Welcome metadata.
    pub(super) commit_sequence_id: i64,
    /// Saved direct, pointer, and pointee envelopes. Retries reuse these bytes.
    #[serde(with = "xmtp_db::byte_strings")]
    pub(super) envelopes: Vec<Vec<u8>>,
    /// Backend receipts for the complete required batch.
    #[serde(default, with = "xmtp_db::optional_byte_strings")]
    pub(super) receipts: Option<Vec<Vec<u8>>>,
}

/// The role of one envelope in a Welcome batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WelcomeEnvelopeKind {
    /// A Welcome wrapped for one installation.
    Inline,
    /// A welcome pointer, which a recipient resolves through the pointee.
    Pointer,
    /// The Welcome that every pointer of the batch names.
    Pointee,
}

/// How a round that sent or settled a Welcome batch ended.
enum Delivery {
    /// The backend acknowledged every envelope of the batch.
    Stored(Vec<EnvelopeMeta>),
    /// The batch cannot be sent, so its intent ends.
    Unsendable,
}

impl PreparedWelcomes {
    /// Each saved envelope as a unit measured against `limits`, or `None` for
    /// an envelope that does not fit in one request under those limits.
    /// Reconstructs the saved batch without generating new keys or ciphertext.
    pub(super) fn units(
        &self,
        limits: &xmtp_configuration::LimitsConfiguration,
    ) -> Result<Vec<Option<PublishUnit>>, GroupError> {
        self.decode()?
            .into_iter()
            .map(
                |envelope| match PublishUnit::single_within(envelope, limits) {
                    Ok(unit) => Ok(Some(unit)),
                    Err(
                        xmtp_api::ApiError::EnvelopeTooLarge | xmtp_api::ApiError::UnitTooLarge,
                    ) => Ok(None),
                    Err(error) => Err(error.into()),
                },
            )
            .collect()
    }

    /// The saved envelopes as units for a recovery read only, one per
    /// envelope. They are not measured against any bound, so never publish
    /// them.
    pub(super) fn settlement_units(&self) -> Result<Vec<PublishUnit>, GroupError> {
        self.decode()?
            .into_iter()
            .map(|envelope| Ok(PublishUnit::for_settlement(vec![envelope])?))
            .collect()
    }

    /// The role of each saved envelope. A pointee is a V1 Welcome whose
    /// wrapper algorithm is `SYMMETRIC_KEY`.
    pub(super) fn kinds(&self) -> Result<Vec<WelcomeEnvelopeKind>, GroupError> {
        self.decode()?
            .into_iter()
            .map(|envelope| {
                let Some(Payload::WelcomeMessage(WelcomeMessageInput {
                    version: Some(version),
                })) = envelope.payload
                else {
                    return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                };
                Ok(match version {
                    WelcomeMessageInputVersion::V1(welcome)
                        if welcome.wrapper_algorithm
                            == i32::from(WelcomeWrapperAlgorithm::SymmetricKey) =>
                    {
                        WelcomeEnvelopeKind::Pointee
                    }
                    WelcomeMessageInputVersion::V1(_) => WelcomeEnvelopeKind::Inline,
                    WelcomeMessageInputVersion::WelcomePointer(_) => WelcomeEnvelopeKind::Pointer,
                })
            })
            .collect()
    }

    /// The saved envelopes. Each carries a Welcome.
    fn decode(&self) -> Result<Vec<ClientEnvelope>, GroupError> {
        if self.envelopes.is_empty() {
            return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
        }
        self.envelopes
            .iter()
            .map(|bytes| {
                let envelope = ClientEnvelope::decode(bytes.as_slice())?;
                if !matches!(envelope.payload, Some(Payload::WelcomeMessage(_))) {
                    return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                }
                Ok(envelope)
            })
            .collect()
    }

    pub(super) fn same_batch(&self, other: &Self) -> bool {
        self.commit_sequence_id == other.commit_sequence_id && self.envelopes == other.envelopes
    }
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Resume required work for committed intents, including after restart.
    /// A commit is not `Processed` until its complete Welcome batch has receipts.
    /// Every committed intent is prepared first, which completes an intent
    /// that owes no Welcomes without any network work; then each saved batch
    /// is sent or settled in id order. One intent's failure does not hold the
    /// others, and the lowest intent's error is returned after the last one.
    //
    // Known limitation: a call waiting on one intent runs this whole step, so
    // it also publishes every earlier intent's follow-up batch. One stalled
    // earlier publish can hold that call past its deadline. Avoiding that
    // needs a durable resume task or a background publish, and either one
    // brings in concurrent runs of this step for the same intents. The gap
    // waiver in docs/specs/waivers.toml tracks it.
    // implements: SEND-024
    pub(in crate::groups::mls_sync) async fn publish_required_welcomes(
        &self,
    ) -> Result<(), GroupError> {
        let ids = self
            .context
            .db()
            .find_group_intents(
                self.group_id,
                Some(vec![IntentState::Committed]),
                Some(IntentKind::all().collect()),
            )?
            .into_iter()
            .map(|intent| intent.id);
        let mut first_error: Option<(i32, GroupError)> = None;
        let mut record = |id: i32, error: GroupError| {
            tracing::warn!(
                group_id = %self.group_id.short_hex(),
                intent_id = id,
                "required Welcomes not published: {error}"
            );
            if first_error.as_ref().is_none_or(|(first, _)| id < *first) {
                first_error = Some((id, error));
            }
        };
        let mut batches = Vec::new();
        for id in ids {
            match self.prepare_required_welcomes(id) {
                Ok(Some(attempt)) => batches.push((id, attempt)),
                Ok(None) => {}
                Err(error) => record(id, error),
            }
        }
        for (id, attempt) in batches {
            if let Err(error) = self.send_required_welcomes(id, &attempt).await {
                record(id, error);
            }
        }
        first_error.map_or(Ok(()), |(_, error)| Err(error))
    }

    /// Publish or settle the saved Welcome batch of committed intent `id`.
    /// Returns `WelcomesUnsendable` when this call ended the intent.
    async fn send_required_welcomes(
        &self,
        id: i32,
        attempt: &PreparedAttempt,
    ) -> Result<(), GroupError> {
        let welcomes = attempt
            .welcomes
            .as_ref()
            .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
        // All crypto and durable state writes finished before this request.
        let units = welcomes.units(self.context.api().limits())?;
        let all_fit = units.iter().cloned().collect::<Option<Vec<_>>>();
        let delivery = match all_fit {
            Some(all) if attempt.unsettled != Some(Unsettled::Welcomes) => {
                match self.context.api().publish_units(all).await {
                    Ok(receipts) => Delivery::Stored(receipts),
                    // A refused request stored nothing and is never sent again.
                    Err(error) if definite_refusal(&error) => Delivery::Unsendable,
                    Err(error) if out_of_range(&error) => {
                        self.mark_unsettled(id, attempt, Unsettled::Welcomes)?;
                        self.settle_welcomes(id, attempt, units).await?
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            // Saved bytes that may be stored, or that no longer fit in one
            // request, are settled by a read before anything else is sent.
            _ => self.settle_welcomes(id, attempt, units).await?,
        };
        match delivery {
            Delivery::Stored(receipts) => self.record_welcome_receipts(id, attempt, receipts),
            Delivery::Unsendable => self.end_follow_up(id, attempt),
        }
    }

    /// Settle saved Welcome bytes that the backend may already hold, because
    /// a publish returned `OUT_OF_RANGE` or because they no longer fit in one
    /// request, with a `Query` of every topic the batch carries. `units` is
    /// [`PreparedWelcomes::units`] under the current limits.
    ///
    /// When every envelope that read does not return fits, those envelopes
    /// are published again with their saved bytes. A deployment carries every
    /// request within its published limits, so an `OUT_OF_RANGE` status came
    /// from a response, and an unstored envelope belonged to a request that
    /// never completed. Otherwise the batch cannot be completed: each
    /// unreturned envelope that fits is published once, except the pointers
    /// to a pointee that does not fit or that the backend refused, and the
    /// batch is unsendable. The batch is never re-encrypted.
    // implements: SEND-007, SEND-023
    async fn settle_welcomes(
        &self,
        id: i32,
        attempt: &PreparedAttempt,
        mut units: Vec<Option<PublishUnit>>,
    ) -> Result<Delivery, GroupError> {
        let welcomes = attempt
            .welcomes
            .as_ref()
            .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
        let api = self.context.api();
        let mut settled = api.settle_units(&welcomes.settlement_units()?).await?;
        let unreturned: Vec<_> = (0..settled.len())
            .filter(|&index| settled[index].is_none())
            .collect();
        if unreturned.iter().all(|&index| units[index].is_some()) {
            if !unreturned.is_empty() {
                let resent = match api
                    .publish_units(
                        unreturned
                            .iter()
                            .filter_map(|&index| units[index].take())
                            .collect(),
                    )
                    .await
                {
                    Ok(resent) => resent,
                    Err(error) if definite_refusal(&error) => return Ok(Delivery::Unsendable),
                    // The next round settles these bytes again, whatever the
                    // limits are by then.
                    Err(error) if out_of_range(&error) => {
                        self.mark_unsettled(id, attempt, Unsettled::Welcomes)?;
                        return Err(error.into());
                    }
                    Err(error) => return Err(error.into()),
                };
                for (index, meta) in unreturned.into_iter().zip(resent) {
                    settled[index] = Some(meta);
                }
            }
            return settled
                .into_iter()
                .collect::<Option<_>>()
                .map(Delivery::Stored)
                .ok_or_else(|| {
                    xmtp_api::ApiError::InvalidResponse("Welcome metadata count").into()
                });
        }
        // implements: SEND-025
        // A recipient whose envelope is stored or still fits can join, so it
        // gets that envelope before the intent ends. A pointer cannot resolve
        // without its pointee, so an unreturned pointee goes out first and
        // alone. The pointers are held back only when the pointee cannot be
        // stored: it does not fit, or the backend refused it. Any other
        // failure can follow a stored pointee. These publishes come first, so
        // a restart before the intent ends reaches the same decision, and
        // their results do not change that decision.
        let kinds = welcomes.kinds()?;
        let pointee_lost = match unreturned
            .iter()
            .find(|&&index| kinds[index] == WelcomeEnvelopeKind::Pointee)
        {
            None => false,
            Some(&index) => match units[index].take() {
                None => true,
                Some(pointee) => self
                    .publish_unsendable(vec![pointee])
                    .await
                    .is_some_and(|error| definite_refusal(&error)),
            },
        };
        let useful: Vec<_> = unreturned
            .into_iter()
            .filter(|&index| !(pointee_lost && kinds[index] == WelcomeEnvelopeKind::Pointer))
            .filter_map(|index| units[index].take())
            .collect();
        if !useful.is_empty() {
            self.publish_unsendable(useful).await;
        }
        Ok(Delivery::Unsendable)
    }

    /// Publish envelopes of a batch that is about to end. A failure is logged
    /// and does not change that outcome; it is returned for the caller to
    /// classify.
    async fn publish_unsendable(&self, units: Vec<PublishUnit>) -> Option<xmtp_api::ApiError> {
        let error = self.context.api().publish_units(units).await.err()?;
        tracing::warn!(
            group_id = %self.group_id.short_hex(),
            "Welcomes of an unsendable batch not published: {error}"
        );
        Some(error)
    }

    /// Save follow-up bytes once, after a positive ordered commit cursor is known.
    /// A committed intent without required work can become processed immediately.
    /// A fresh batch that does not fit under the current limits was never
    /// sent, so this writer ends the intent without saving the batch and the
    /// call returns `WelcomesUnsendable`.
    pub(super) fn prepare_required_welcomes(
        &self,
        id: i32,
    ) -> Result<Option<PreparedAttempt>, GroupError> {
        let limits = self.context.api().limits();
        // An ended intent is committed with the writer; its error is returned
        // after it.
        crate::state_tx::state_write_durable(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let Some(intent) = Fetch::<StoredGroupIntent>::fetch(&db, &id)? else {
                return Ok(Continue(Ok(None)));
            };
            if intent.group_id != self.group_id || intent.state != IntentState::Committed {
                return Ok(Continue(Ok(None)));
            }
            let Some(post_commit_data) = intent.post_commit_data.as_deref() else {
                db.set_group_intent_processed(id)?;
                return Ok(Continue(Ok(None)));
            };
            let commit_sequence_id = intent
                .sequence_id
                .filter(|sequence_id| *sequence_id > 0)
                .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
            let encoded = db
                .prepared_envelopes(id)?
                .ok_or(OutgoingPreparationError::MissingPreparedAttempt(id))?;
            let mut attempt = PreparedAttempt::decode(&encoded)?;
            attempt.validate_intent(&intent)?;
            if let Some(welcomes) = &attempt.welcomes {
                if welcomes.commit_sequence_id != commit_sequence_id {
                    return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                }
                // Shape only: saved bytes that no longer fit are settled
                // before any current limit applies to them.
                welcomes.settlement_units()?;
                if let Some(receipts) = &welcomes.receipts {
                    if receipts.len() != welcomes.envelopes.len() {
                        return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                    }
                    db.set_group_intent_processed(id)?;
                    return Ok(Continue(Ok(None)));
                }
                return Ok(Continue(Ok(Some(attempt))));
            }
            let PostCommitAction::SendWelcomes(action) =
                PostCommitAction::from_bytes(post_commit_data)?;
            let envelopes = self.prepare_welcome_envelopes(action, commit_sequence_id as u64)?;
            let welcomes = PreparedWelcomes {
                commit_sequence_id,
                envelopes: envelopes.iter().map(Message::encode_to_vec).collect(),
                receipts: None,
            };
            // implements: SEND-025
            if welcomes.units(limits)?.iter().any(Option::is_none) {
                db.set_group_intent_follow_up_failed(id)?;
                return Ok(Continue(Err(OutgoingPreparationError::WelcomesUnsendable)));
            }
            attempt.welcomes = Some(welcomes);
            attempt.unsettled = None;
            let replacement = xmtp_db::db_serialize(&attempt)?;
            if !db.compare_and_set_prepared_envelopes(id, Some(&encoded), Some(&replacement))? {
                return Err(OutgoingPreparationError::StateChanged.into());
            }
            Ok::<_, GroupError>(Continue(Ok(Some(attempt))))
        })?
        .into_continued()
        .map_err(Into::into)
    }

    /// Under the caller's writer, the saved attempt of committed intent `id`
    /// and its encoded bytes, while it is still `sent` with the same Welcome
    /// batch at the intent's commit cursor. `None` when another writer
    /// resolved, replaced, or removed it.
    fn current_welcome_batch(
        &self,
        db: &impl DbQuery,
        id: i32,
        sent: &PreparedAttempt,
    ) -> Result<Option<(Vec<u8>, PreparedAttempt)>, GroupError> {
        let sent_welcomes = sent
            .welcomes
            .as_ref()
            .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
        let Some(intent) = Fetch::<StoredGroupIntent>::fetch(db, &id)? else {
            return Ok(None);
        };
        if intent.group_id != self.group_id || intent.state != IntentState::Committed {
            return Ok(None);
        }
        let Some(encoded) = db.prepared_envelopes(id)? else {
            return Ok(None);
        };
        let current = PreparedAttempt::decode(&encoded)?;
        if !current.same_attempt(sent) {
            return Ok(None);
        }
        current.validate_intent(&intent)?;
        let matches = current.welcomes.as_ref().is_some_and(|welcomes| {
            welcomes.same_batch(sent_welcomes)
                && intent.sequence_id == Some(welcomes.commit_sequence_id)
        });
        Ok(matches.then_some((encoded, current)))
    }

    /// Mark the intent processed only if both the attempt and Welcome batch still match.
    pub(super) fn record_welcome_receipts(
        &self,
        id: i32,
        sent_attempt: &PreparedAttempt,
        receipts: Vec<EnvelopeMeta>,
    ) -> Result<(), GroupError> {
        let sent_welcomes = sent_attempt
            .welcomes
            .as_ref()
            .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
        if receipts.len() != sent_welcomes.envelopes.len() {
            return Err(xmtp_api::ApiError::InvalidResponse("Welcome metadata count").into());
        }
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let Some((encoded, mut current)) = self.current_welcome_batch(&db, id, sent_attempt)?
            else {
                return Ok(Continue(()));
            };
            let welcomes = current
                .welcomes
                .as_mut()
                .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
            let receipt_bytes = receipts
                .iter()
                .map(Message::encode_to_vec)
                .collect::<Vec<_>>();
            if let Some(existing) = &welcomes.receipts {
                if existing != &receipt_bytes {
                    return Err(xmtp_api::ApiError::InvalidResponse(
                        "conflicting Welcome metadata",
                    )
                    .into());
                }
            } else {
                welcomes.receipts = Some(receipt_bytes);
            }
            current.unsettled = None;
            let replacement = xmtp_db::db_serialize(&current)?;
            if db.compare_and_set_prepared_envelopes(id, Some(&encoded), Some(&replacement))? {
                db.set_group_intent_processed(id)?;
            }
            Ok::<_, GroupError>(Continue(()))
        })?;
        Ok(())
    }

    /// End the intent as `FollowUpFailed`, only while it still holds the attempt
    /// and Welcome batch that were sent and the batch has no receipts. The
    /// saved attempt stays as it is. Returns `WelcomesUnsendable` when this
    /// call ended the intent.
    // implements: SEND-025
    pub(super) fn end_follow_up(
        &self,
        id: i32,
        sent_attempt: &PreparedAttempt,
    ) -> Result<(), GroupError> {
        let ended = crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let open = self
                .current_welcome_batch(&db, id, sent_attempt)?
                .is_some_and(|(_, current)| {
                    current
                        .welcomes
                        .is_some_and(|welcomes| welcomes.receipts.is_none())
                });
            if open {
                db.set_group_intent_follow_up_failed(id)?;
            }
            Ok::<_, GroupError>(Continue(open))
        })?
        .into_continued();
        if ended {
            return Err(OutgoingPreparationError::WelcomesUnsendable.into());
        }
        Ok(())
    }

    /// Build all direct, pointer, and pointee bytes under the caller's writer.
    pub(in crate::groups::mls_sync) fn prepare_welcome_envelopes(
        &self,
        action: SendWelcomesAction,
        message_cursor: u64,
    ) -> Result<Vec<ClientEnvelope>, GroupError> {
        // Only encode welcome metadata once
        let welcome_metadata = WelcomeMetadata { message_cursor };
        let welcome_metadata_bytes = welcome_metadata.encode_to_vec();

        // The threshold counts the same installations the loop below sends
        // pointers to, so a pointee is built only when enough of them use it.
        let wp_capable = action
            .installations
            .iter()
            .filter(|installation| installation.pointer_capable())
            .count();

        let (welcome_pointer_bytes, welcome_pointee) = if wp_capable
            > xmtp_configuration::INSTALLATION_THRESHOLD_FOR_WELCOME_POINTER_SENDING
        {
            let destination = xmtp_common::rand_array::<32>();
            tracing::debug!(
                wp_capable,
                destination = %hex::encode(destination),
                "Using welcome pointers"
            );
            use xmtp_configuration::{WELCOME_POINTEE_KEY_BYTES, WELCOME_POINTEE_NONCE_BYTES};
            let symmetric_key =
                Zeroizing::new(xmtp_common::rand_array::<WELCOME_POINTEE_KEY_BYTES>());
            let data_nonce =
                Zeroizing::new(xmtp_common::rand_array::<WELCOME_POINTEE_NONCE_BYTES>());
            let mut welcome_metadata_nonce =
                Zeroizing::new(xmtp_common::rand_array::<WELCOME_POINTEE_NONCE_BYTES>());
            // Recipients reject a pointer whose two nonces are equal.
            while welcome_metadata_nonce == data_nonce {
                welcome_metadata_nonce =
                    Zeroizing::new(xmtp_common::rand_array::<WELCOME_POINTEE_NONCE_BYTES>());
            }

            let aead_type = crate::groups::mls_ext::WelcomePointersExtension::preferred_type();
            let data = wrap_payload_symmetric(
                &action.welcome_message,
                aead_type,
                symmetric_key.as_ref(),
                data_nonce.as_ref(),
            )?;
            let welcome_metadata = wrap_payload_symmetric(
                &welcome_metadata_bytes,
                aead_type,
                symmetric_key.as_ref(),
                welcome_metadata_nonce.as_ref(),
            )?;

            let welcome_pointee = WelcomeMessageInput {
                version: Some(WelcomeMessageInputVersion::V1(WelcomeMessageInputV1 {
                    installation_key: destination.into(),
                    data,
                    hpke_public_key: vec![],
                    wrapper_algorithm: xmtp_proto::xmtp::mls::message_contents::WelcomeWrapperAlgorithm::SymmetricKey.into(),
                    welcome_metadata,
                })),
            };
            let welcome_pointer_bytes = Zeroizing::new(WelcomePointerProto {
                version: Some(
                    xmtp_proto::xmtp::mls::message_contents::welcome_pointer::Version::WelcomeV1Pointer(
                        xmtp_proto::xmtp::mls::message_contents::welcome_pointer::WelcomeV1Pointer {
                            destination: destination.into(),
                            aead_type: xmtp_proto::xmtp::mls::message_contents::WelcomePointeeEncryptionAeadType::Chacha20Poly1305.into(),
                            encryption_key: symmetric_key.as_ref().to_vec(),
                            data_nonce: data_nonce.as_ref().to_vec(),
                            welcome_metadata_nonce: welcome_metadata_nonce.as_ref().to_vec(),
                        },
                    ),
                ),
            }.encode_to_vec());

            (Some(welcome_pointer_bytes), Some(welcome_pointee))
        } else {
            (None, None)
        };

        let total_installations = action.installations.len();

        let welcomes_iter = action.installations.into_iter().map(
            |installation| -> Result<WelcomeMessageInput, WrapPayloadError> {
                let algorithm = installation.welcome_wrapper_algorithm;
                // A pointer-capable installation's wrapper is post-quantum, so its
                // value is also the pointer's wrapper value.
                if let Some(welcome_pointer) = &welcome_pointer_bytes
                    && installation.pointer_capable()
                {
                    Ok(WelcomeMessageInput {
                        version: Some(WelcomeMessageInputVersion::WelcomePointer(
                            WelcomePointerInput {
                                installation_key: installation.installation_key,
                                welcome_pointer: wrap_payload_hpke(
                                    welcome_pointer.as_ref(),
                                    &[],
                                    &installation.hpke_public_key,
                                    algorithm,
                                    WELCOME_HPKE_LABEL,
                                )?
                                .0,
                                hpke_public_key: installation.hpke_public_key,
                                wrapper_algorithm: algorithm.into(),
                            },
                        )),
                    })
                } else {
                    let installation_key = installation.installation_key;

                    let (data, welcome_metadata) = wrap_payload_hpke(
                        &action.welcome_message,
                        &welcome_metadata_bytes,
                        &installation.hpke_public_key,
                        algorithm,
                        WELCOME_HPKE_LABEL,
                    )?;
                    Ok(WelcomeMessageInput {
                        version: Some(WelcomeMessageInputVersion::V1(WelcomeMessageInputV1 {
                            installation_key,
                            data,
                            hpke_public_key: installation.hpke_public_key,
                            wrapper_algorithm: algorithm.into(),
                            welcome_metadata,
                        })),
                    })
                }
            },
        );

        let welcomes = welcome_pointee
            .into_iter()
            .map(Ok)
            .chain(welcomes_iter)
            .collect::<Result<Vec<WelcomeMessageInput>, WrapPayloadError>>()?;

        assert_eq!(
            welcomes.len(),
            total_installations + usize::from(welcome_pointer_bytes.is_some())
        );

        if welcomes.is_empty() {
            return Err(GroupError::NoWelcomesToSend);
        }
        Ok(welcomes
            .into_iter()
            .map(|welcome| ClientEnvelope {
                payload: Some(Payload::WelcomeMessage(welcome)),
            })
            .collect())
    }
}
