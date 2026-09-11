//! Required Welcome publication after an ordered commit.

use super::*;
use prost::Message;
use serde::{Deserialize, Serialize};
use xmtp_proto::backend_v1::EnvelopeMeta;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// Exact follow-up bytes anchored to one accepted membership commit.
pub(crate) struct PreparedWelcomes {
    /// Ordered commit cursor included in the Welcome metadata.
    pub(super) commit_sequence_id: i64,
    /// Saved direct, pointer, and pointee envelopes. Retries reuse these bytes.
    pub(super) envelopes: Vec<Vec<u8>>,
    /// Backend receipts for the complete required batch.
    pub(super) receipts: Option<Vec<Vec<u8>>>,
}

impl PreparedWelcomes {
    /// Reconstruct the saved batch without generating new keys or ciphertext.
    pub(super) fn units(&self) -> Result<Vec<PublishUnit>, GroupError> {
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
                Ok(PublishUnit::single(envelope)?)
            })
            .collect()
    }

    fn same_batch(&self, other: &Self) -> bool {
        self.commit_sequence_id == other.commit_sequence_id && self.envelopes == other.envelopes
    }
}

impl<Context: XmtpSharedContext> MlsGroup<Context> {
    /// Resume required work for committed intents, including after restart.
    /// A commit is not `Processed` until its complete Welcome batch has receipts.
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
            .map(|intent| intent.id)
            .collect::<Vec<_>>();
        for id in ids {
            let Some(attempt) = self.prepare_required_welcomes(id)? else {
                continue;
            };
            let welcomes = attempt
                .welcomes
                .as_ref()
                .ok_or(OutgoingPreparationError::InvalidPreparedAttempt)?;
            // All crypto and durable state writes finished before this request.
            let receipts = self.context.api().publish_units(welcomes.units()?).await?;
            self.record_welcome_receipts(id, &attempt, receipts)?;
        }
        Ok(())
    }

    /// Save follow-up bytes once, after a positive ordered commit cursor is known.
    /// A committed intent without required work can become processed immediately.
    pub(super) fn prepare_required_welcomes(
        &self,
        id: i32,
    ) -> Result<Option<PreparedAttempt>, GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            let Some(intent) = Fetch::<StoredGroupIntent>::fetch(&db, &id)? else {
                return Ok(Continue(None));
            };
            if intent.group_id != self.group_id || intent.state != IntentState::Committed {
                return Ok(Continue(None));
            }
            let Some(post_commit_data) = intent.post_commit_data.as_deref() else {
                db.set_group_intent_processed(id)?;
                return Ok(Continue(None));
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
                welcomes.units()?;
                if let Some(receipts) = &welcomes.receipts {
                    if receipts.len() != welcomes.envelopes.len() {
                        return Err(OutgoingPreparationError::InvalidPreparedAttempt.into());
                    }
                    db.set_group_intent_processed(id)?;
                    return Ok(Continue(None));
                }
                return Ok(Continue(Some(attempt)));
            }
            let PostCommitAction::SendWelcomes(action) =
                PostCommitAction::from_bytes(post_commit_data)?;
            let envelopes = self.prepare_welcome_envelopes(action, commit_sequence_id as u64)?;
            let welcomes = PreparedWelcomes {
                commit_sequence_id,
                envelopes: envelopes.iter().map(Message::encode_to_vec).collect(),
                receipts: None,
            };
            welcomes.units()?;
            attempt.welcomes = Some(welcomes);
            let replacement = xmtp_db::db_serialize(&attempt)?;
            if !db.compare_and_set_prepared_envelopes(id, Some(&encoded), Some(&replacement))? {
                return Err(OutgoingPreparationError::StateChanged.into());
            }
            Ok::<_, GroupError>(Continue(Some(attempt)))
        })
        .map(TransactionOutcome::into_continued)
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
            let Some(intent) = Fetch::<StoredGroupIntent>::fetch(&db, &id)? else {
                return Ok(Continue(()));
            };
            if intent.group_id != self.group_id || intent.state != IntentState::Committed {
                return Ok(Continue(()));
            }
            let Some(encoded) = db.prepared_envelopes(id)? else {
                return Ok(Continue(()));
            };
            let mut current = PreparedAttempt::decode(&encoded)?;
            if !current.same_attempt(sent_attempt) {
                return Ok(Continue(()));
            }
            current.validate_intent(&intent)?;
            let Some(welcomes) = current.welcomes.as_mut() else {
                return Ok(Continue(()));
            };
            if !welcomes.same_batch(sent_welcomes)
                || intent.sequence_id != Some(welcomes.commit_sequence_id)
            {
                return Ok(Continue(()));
            }
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
            let replacement = xmtp_db::db_serialize(&current)?;
            if db.compare_and_set_prepared_envelopes(id, Some(&encoded), Some(&replacement))? {
                db.set_group_intent_processed(id)?;
            }
            Ok::<_, GroupError>(Continue(()))
        })?;
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

        let wp_capable = action
            .installations
            .iter()
            .filter(|installation| {
                installation
                    .welcome_pointee_encryption_aead_types
                    .compatible()
            })
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
            let symmetric_key = Zeroizing::new(xmtp_common::rand_array::<32>());
            let data_nonce = Zeroizing::new(xmtp_common::rand_array::<12>());
            let mut welcome_metadata_nonce = Zeroizing::new(xmtp_common::rand_array::<12>());
            // ensure that the welcome pointer nonce is different from the data nonce
            while welcome_metadata_nonce == data_nonce {
                welcome_metadata_nonce = Zeroizing::new(xmtp_common::rand_array::<12>());
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
                // Unconditionally use the wrapper algorithm for the welcome pointer because it will always be post quantum compatible.
                let algorithm = installation.welcome_wrapper_algorithm;
                let wp_cap = installation.welcome_pointee_encryption_aead_types;
                if let Some(welcome_pointer) = &welcome_pointer_bytes
                    && wp_cap.compatible()
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
