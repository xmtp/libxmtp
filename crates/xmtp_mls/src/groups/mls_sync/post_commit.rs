//! Post-commit work: installations, welcomes, and HMAC keys.

use super::*;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    #[tracing::instrument(skip_all)]
    pub(crate) async fn post_commit(&self) -> Result<(), GroupError> {
        self.publish_required_welcomes().await
    }

    pub async fn maybe_update_installations(
        &self,
        update_interval_ns: Option<i64>,
    ) -> Result<(), GroupError> {
        let db = self.context.db();
        let Some(stored_group) = db.find_group(&self.group_id)? else {
            return Err(GroupError::NotFound(NotFound::GroupById(self.group_id)));
        };
        if stored_group.conversation_type.is_virtual() {
            return Ok(());
        }

        // determine how long of an interval in time to use before updating list
        let interval_ns = update_interval_ns.unwrap_or(SYNC_UPDATE_INSTALLATIONS_INTERVAL_NS);

        let now_ns = xmtp_common::time::now_ns();
        let last_ns = db.get_installations_time_checked(&self.group_id)?;
        let elapsed_ns = now_ns - last_ns;
        if elapsed_ns > interval_ns && self.is_active()? {
            self.add_missing_installations().await?;
            db.update_installations_time_checked(&self.group_id)?;
        }

        Ok(())
    }

    /**
     * Checks each member of the group for `IdentityUpdates` after their current sequence_id. If updates
     * are found the method will construct an [`UpdateGroupMembershipIntentData`] and create a change
     * to the [`GroupMembership`] that will add any missing installations.
     *
     * This is designed to handle cases where existing members have added a new installation to their inbox or revoked an installation
     * and the group has not been updated to include it.
     */
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip_all))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub(crate) async fn add_missing_installations(&self) -> Result<(), GroupError> {
        let intent_data = self.get_membership_update_intent(&[], &[]).await?;

        // If there is nothing to do, stop here
        if intent_data.is_empty() {
            return Ok(());
        }

        debug!(
            inbox_id = self.context.inbox_id(),
            installation_id = %self.context.installation_id(),
            "Adding missing installations {:?}",
            intent_data
        );

        let intent = QueueIntent::update_group_membership()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip_all)]
    /**
     * get_membership_update_intent will query the network for any new [`IdentityUpdate`]s for any of the existing
     * group members
     *
     * Callers may also include a list of added or removed inboxes
     */
    pub(crate) async fn get_membership_update_intent(
        &self,
        inbox_ids_to_add: &[InboxIdRef<'_>],
        inbox_ids_to_remove: &[InboxIdRef<'_>],
    ) -> Result<UpdateGroupMembershipIntentData, GroupError> {
        let existing_group_membership = self.with_group_snapshot(|group| {
            extract_group_membership(group.extensions()).map_err(Into::into)
        })?;
        {
            // TODO:nm prevent querying for updates on members who are being removed
            let mut inbox_ids = existing_group_membership.inbox_ids();
            inbox_ids.extend_from_slice(inbox_ids_to_add);
            let conn = self.context.db();
            // Load any missing updates from the network
            load_identity_updates(self.context.api(), &conn, &inbox_ids).await?;

            let latest_sequence_id_map = conn.get_latest_sequence_id(&inbox_ids as &[&str])?;

            // Get a list of all inbox IDs that have increased sequence_id for the group
            let changed_inbox_ids =
                inbox_ids
                    .iter()
                    .try_fold(HashMap::new(), |mut updates, inbox_id| {
                        match (
                            latest_sequence_id_map.get(inbox_id as &str),
                            existing_group_membership.get(inbox_id),
                        ) {
                            // This is an update. We have a new sequence ID and an existing one
                            (Some(latest_sequence_id), Some(current_sequence_id)) => {
                                let latest_sequence_id_u64 = *latest_sequence_id as u64;
                                if latest_sequence_id_u64.gt(current_sequence_id) {
                                    updates.insert(inbox_id.to_string(), latest_sequence_id_u64);
                                }
                            }
                            // This is for new additions to the group
                            (Some(latest_sequence_id), None) => {
                                // This is the case for net new members to the group
                                updates.insert(inbox_id.to_string(), *latest_sequence_id as u64);
                            }
                            (_, _) => {
                                tracing::warn!(
                                    "Could not find existing sequence ID for inbox {}",
                                    inbox_id
                                );
                                return Err(GroupError::MissingSequenceId);
                            }
                        }

                        Ok(updates)
                    })?;
            let old_group_membership = existing_group_membership.clone();
            let mut new_membership = old_group_membership.clone();
            for (inbox_id, sequence_id) in changed_inbox_ids.iter() {
                new_membership.add(inbox_id.clone(), *sequence_id);
            }
            for inbox_id in inbox_ids_to_remove {
                new_membership.remove(inbox_id);
            }

            let changes_with_kps = calculate_membership_changes_with_keypackages(
                &self.context,
                &self.group_id,
                &new_membership,
                &old_group_membership,
            )
            .await?;

            // If we fail to fetch or verify all the added members' KeyPackage, return an error.
            // skip if the inbox ids is 0 from the beginning
            if !inbox_ids_to_add.is_empty()
                && !changes_with_kps.failed_installations.is_empty()
                && changes_with_kps.new_installations.is_empty()
            {
                return Err(GroupError::FailedToVerifyInstallations(
                    FailedInstallationIds(changes_with_kps.failed_installations.clone()),
                ));
            }

            Ok(UpdateGroupMembershipIntentData::new(
                changed_inbox_ids,
                inbox_ids_to_remove
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<String>>(),
                changes_with_kps.failed_installations,
            ))
        }
    }

    #[cfg(test)]
    pub(in crate::groups) async fn send_welcomes(
        &self,
        action: SendWelcomesAction,
        message_cursor: Option<i64>,
    ) -> Result<(), GroupError> {
        let message_cursor = u64::try_from(message_cursor.unwrap_or(0))
            .map_err(|_| xmtp_proto::ConversionError::Unspecified("negative Welcome cursor"))?;
        let units = crate::state_tx::state_write(self.context.mls_storage(), |_tx| {
            self.prepare_welcome_envelopes(action, message_cursor)?
                .into_iter()
                .map(PublishUnit::single)
                .collect::<Result<Vec<_>, _>>()
                .map(Continue)
                .map_err(GroupError::from)
        })?
        .into_continued();
        self.context.api().publish_units(units).await?;
        Ok(())
    }

    /// Provides hmac keys for a range of epochs around current epoch
    /// `group.hmac_keys(-1..=1)`` will provide 3 keys consisting of last epoch, current epoch, and next epoch
    /// `group.hmac_keys(0..=0) will provide 1 key, consisting of only the current epoch
    #[tracing::instrument(level = "trace", skip_all)]
    pub fn hmac_keys(
        &self,
        epoch_delta_range: RangeInclusive<i64>,
    ) -> Result<Vec<HmacKey>, StorageError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            self.hmac_keys_in(tx.storage().db(), epoch_delta_range)
                .map(Continue)
        })
        .map(TransactionOutcome::into_continued)
    }

    pub(crate) fn hmac_keys_in(
        &self,
        conn: impl xmtp_db::DbQuery,
        epoch_delta_range: RangeInclusive<i64>,
    ) -> Result<Vec<HmacKey>, StorageError> {
        let preferences = StoredUserPreferences::load(&conn)?;
        let mut ikm = match preferences.hmac_key {
            Some(ikm) => ikm,
            None => {
                let key = HmacKey::random_key();
                StoredUserPreferences::store_hmac_key(&conn, &key, None)?;
                key
            }
        };
        ikm.extend_from_slice(self.group_id.as_ref());
        let hkdf = Hkdf::<Sha256>::new(Some(HMAC_SALT), &ikm);

        let mut result = vec![];
        let current_epoch = hmac_epoch();
        for delta in epoch_delta_range {
            let epoch = current_epoch + delta;

            let mut info = self.group_id.to_vec();
            info.extend(&epoch.to_le_bytes());

            let mut key = [0; 42];
            hkdf.expand(&info, &mut key).expect("Length is correct");

            result.push(HmacKey { key, epoch });
        }

        Ok(result)
    }

    #[cfg(test)]
    #[tracing::instrument(level = "trace", skip_all)]
    pub(in crate::groups) fn prepare_group_messages(
        &self,
        payloads: Vec<(&[u8], bool)>,
    ) -> Result<Vec<PublishUnit>, GroupError> {
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let envelopes = self.prepare_group_envelopes_in(tx.storage().db(), payloads)?;
            Ok::<_, GroupError>(Continue(vec![PublishUnit::new(envelopes)?]))
        })
        .map(TransactionOutcome::into_continued)
    }

    pub(super) fn prepare_group_envelopes_in(
        &self,
        conn: impl xmtp_db::DbQuery,
        payloads: Vec<(&[u8], bool)>,
    ) -> Result<Vec<ClientEnvelope>, GroupError> {
        let hmac_key = self
            .hmac_keys_in(conn, 0..=0)?
            .pop()
            .expect("Range of count 1 was provided.");
        let sender_hmac =
            Hmac::<Sha256>::new_from_slice(&hmac_key.key).expect("HMAC can take key of any size");

        let mut result = vec![];
        for (payload, should_push) in payloads {
            let mut sender_hmac = sender_hmac.clone();
            sender_hmac.update(payload);
            let sender_hmac = sender_hmac.finalize();

            result.push(ClientEnvelope {
                payload: Some(Payload::GroupMessage(BackendGroupMessage {
                    data: payload.to_vec(),
                    sender_hmac: sender_hmac.into_bytes().to_vec(),
                    should_push,
                })),
            });
        }

        Ok(result)
    }
}
