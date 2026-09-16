//! Adding, removing, and re-adding members.

use super::*;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    ///
    /// Add members to the group by account address
    ///
    /// If any existing members have new installations that have not been added or removed, the
    /// group membership will be updated to include those changes as well.
    /// # Returns
    /// - `Ok(UpdateGroupMembershipResult)`: Contains details about the membership changes, including:
    ///   - `added_members`: list of added installations
    ///   - `removed_members`: A list of installations that were removed.
    ///   - `members_with_errors`: A list of members that encountered errors during the update.
    /// - `Err(GroupError)`: If the operation fails due to an error.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn add_members_by_identity(
        &self,
        account_identifiers: &[Identifier],
    ) -> Result<UpdateGroupMembershipResult, GroupError> {
        // Fetch the associated inbox_ids
        let requests = account_identifiers.iter().map(Into::into).collect();
        let inbox_id_map: HashMap<Identifier, String> = self
            .context
            .api()
            .get_inbox_ids(requests)
            .await?
            .into_iter()
            .zip(account_identifiers.iter().cloned())
            .filter_map(|(inbox, identifier)| inbox.map(|inbox| (identifier, inbox)))
            .collect();

        // get current number of users in group
        let member_count = self.members().await?.len();
        // CFG-066: the deployment sets the ceiling, checked before the commit
        // is built and before anything is published.
        let max_members = self
            .context
            .server_configuration()
            .configuration()
            .mls
            .max_group_members;
        if member_count + inbox_id_map.len() > max_members {
            return Err(GroupError::UserLimitExceeded);
        }

        if inbox_id_map.len() != account_identifiers.len() {
            let found_addresses: HashSet<&Identifier> = inbox_id_map.keys().collect();
            let to_add_hashset = HashSet::from_iter(account_identifiers.iter());

            let missing_addresses = found_addresses.difference(&to_add_hashset);
            return Err(GroupError::AddressNotFound(
                missing_addresses
                    .into_iter()
                    .map(|ident| format!("{ident}"))
                    .collect(),
            ));
        }

        self.add_members(&inbox_id_map.into_values().collect::<Vec<_>>())
            .await
    }

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", skip_all, fields(inbox_id = %self.context.inbox_id(), inbox_ids = ?inbox_ids.as_ref().iter().map(|i| i.as_ref()).collect::<Vec<_>>())))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub async fn add_members<S: AsIdRef>(
        &self,
        inbox_ids: impl AsRef<[S]>,
    ) -> Result<UpdateGroupMembershipResult, GroupError> {
        self.ensure_not_paused().await?;

        let ids = inbox_ids
            .as_ref()
            .iter()
            .map(AsIdRef::as_ref)
            .collect::<Vec<&str>>();
        let intent_data = self
            .get_membership_update_intent(ids.as_slice(), &[])
            .await?;

        // TODO:nm this isn't the best test for whether the request is valid
        // If some existing group member has an update, this will return an intent with changes
        // when we really should return an error
        let ok_result = Ok(UpdateGroupMembershipResult::from(intent_data.clone()));

        if intent_data.is_empty() {
            tracing::warn!("Member already added");
            return ok_result;
        }

        let intent = QueueIntent::update_group_membership()
            .data(intent_data)
            .queue(self)?;

        self.sync_until_intent_resolved(intent.id).await?;
        let epoch = self.epoch().await?;

        log_event!(
            Event::AddedMembers,
            self.context.installation_id(),
            group_id = self.group_id,
            members = ?ids,
            epoch
        );

        ok_result
    }

    /// Removes members from the group by their account addresses.
    ///
    /// # Arguments
    /// * `client` - The XMTP client.
    /// * `account_addresses_to_remove` - A vector of account addresses to remove from the group.
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    pub async fn remove_members_by_identity(
        &self,
        account_addresses_to_remove: &[Identifier],
    ) -> Result<(), GroupError> {
        let account_addresses_to_remove =
            account_addresses_to_remove.iter().map(Into::into).collect();

        let inbox_id_map = self
            .context
            .api()
            .get_inbox_ids(account_addresses_to_remove)
            .await?;

        let ids = inbox_id_map
            .iter()
            .flatten()
            .map(AsRef::as_ref)
            .collect::<Vec<&str>>();
        self.remove_members(ids.as_slice()).await
    }

    /// Removes members from the group by their inbox IDs.
    ///
    /// # Arguments
    /// * `client` - The XMTP client.
    /// * `inbox_ids` - A vector of inbox IDs to remove from the group.
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", skip_all, fields(inbox_id = %self.context.inbox_id(), inbox_ids = ?inbox_ids)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip_all)
    )]
    pub async fn remove_members(&self, inbox_ids: &[InboxIdRef<'_>]) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;
        let intent_data = self.get_membership_update_intent(&[], inbox_ids).await?;
        let intent = QueueIntent::update_group_membership()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;

        Ok(())
    }

    /// Removes and readds installations from the MLS tree.
    ///
    /// The installation list should be validated beforehand - invalid installations
    /// will simply be omitted at the time that the intent's publish data is computed.
    ///
    /// # Arguments
    /// * `installations` - A vector of installations to readd.
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    #[allow(dead_code)]
    pub(crate) async fn readd_installations(
        &self,
        installations: Vec<Vec<u8>>,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let readd_min_version =
            LibXMTPVersion::parse(xmtp_configuration::MIN_RECOVERY_REQUEST_VERSION)?;
        let metadata = self.mutable_metadata()?;
        let group_version = metadata
            .attributes
            .get(MetadataField::MinimumSupportedProtocolVersion.as_str());
        let group_min_version =
            LibXMTPVersion::parse(group_version.unwrap_or(&"0.0.0".to_string()))?;

        if readd_min_version > group_min_version {
            self.update_group_min_version(xmtp_configuration::MIN_RECOVERY_REQUEST_VERSION)
                .await?;
        }

        let intent_data: Vec<u8> = ReaddInstallationsIntentData::new(installations.clone()).into();
        let intent = QueueIntent::readd_installations()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;

        Ok(())
    }

    /// Process this group's pending self-remove requests end-to-end: remove the
    /// members still in the group that requested removal, then clean up stale
    /// pending-remove rows. Idempotent and a no-op when this client is not a
    /// super-admin, so it is safe to call from both the inline message-processing
    /// fast-path and the durable `TaskRunner` retry path.
    pub(crate) async fn process_pending_self_removals(&self) -> Result<(), GroupError> {
        // Both helpers early-return on an empty pending list; cleanup owns the
        // flag-clear. Keeping the empty-check inside cleanup (rather than a
        // separate clear here) avoids racing a concurrent LeaveRequest insert and
        // wrongly clearing the flag.
        self.remove_members_pending_removal().await?;
        self.cleanup_pending_removal_list().await?;
        Ok(())
    }

    /// Removes all members from the group who are currently in the pending removal list.
    ///
    /// Only admins and super admins can call this function. Validates permissions, filters
    /// out invalid removal requests and performs batch removal of valid pending members.
    ///
    /// # Returns
    /// * `Ok(())` - All valid pending members were successfully removed
    /// * `Err(GroupError)` - Failed to retrieve metadata, validate permissions or execute removals
    pub async fn remove_members_pending_removal(&self) -> Result<(), GroupError> {
        let pending_removal_list = self.pending_remove_list()?;

        if pending_removal_list.is_empty() {
            tracing::debug!(
                group_id = %self.group_id,
                inbox_id = %self.context.inbox_id(),
                "Group has no pending removal members"
            );
            return Ok(());
        }

        let is_super_admin = self.is_super_admin(self.context.inbox_id().to_string())?;
        if !is_super_admin {
            tracing::debug!(
                group_id = %self.group_id,
                inbox_id = %self.context.inbox_id(),
                "Current inbox ID is not in admin or super admin list, skipping pending removal processing"
            );
            return Ok(());
        }

        // Get current group members to validate which ones actually exist
        let members = self.members().await?;
        let member_inbox_ids: HashSet<String> =
            members.iter().map(|m| m.inbox_id.clone()).collect();

        // Filter pending removals to only include actual group members
        let valid_removals: Vec<&str> = pending_removal_list
            .iter()
            .filter(|inbox_id| member_inbox_ids.contains(*inbox_id))
            .map(|s| s.as_str())
            .collect();

        if valid_removals.is_empty() {
            tracing::warn!(
                group_id = %self.group_id,
                pending_count = pending_removal_list.len(),
                "No valid members found in pending removal list"
            );
            return Ok(());
        }
        // Log members that are in pending list but not in group
        let invalid_removals: Vec<&String> = pending_removal_list
            .iter()
            .filter(|inbox_id| !member_inbox_ids.contains(*inbox_id))
            .collect();

        if !invalid_removals.is_empty() {
            tracing::warn!(
                group_id = %self.group_id,
                invalid_members = ?invalid_removals,
                "Some members in pending removal list are not in the group"
            );
        }

        // Remove all valid members at once
        tracing::info!(
            group_id = %self.group_id,
            removing_count = valid_removals.len(),
            members_to_remove = ?valid_removals,
            "Removing pending members from group"
        );

        match self.remove_members(&valid_removals).await {
            Ok(_) => {
                tracing::info!(
                    group_id = %self.group_id,
                    removed_count = valid_removals.len(),
                    removed_members = ?valid_removals,
                    "Successfully removed all pending members from group"
                );
            }
            Err(e) => {
                tracing::error!(
                    group_id = %self.group_id,
                    removed_members = ?valid_removals,
                    error = %e,
                    "Failed to remove pending members from group"
                );
                return Err(e);
            }
        }

        Ok(())
    }

    /// Removes members from the pending removal list who are no longer in the group.
    ///
    /// Iterates through all members in the pending removal list, checking each one to see
    /// if they're still in the group. If a member is no longer in the group, they are
    /// removed from the pending list. The pending list is refreshed after each removal
    /// to ensure we're working with the most current data.
    ///
    /// # Returns
    /// * `Ok(())` - Successfully processed all pending removal members
    /// * `Err(GroupError)` - Failed to retrieve data or update the pending list
    pub async fn cleanup_pending_removal_list(&self) -> Result<(), GroupError> {
        tracing::debug!(
            group_id = %self.group_id,
            "Starting pending removal list cleanup"
        );

        // Get both lists upfront
        let pending_removal_list = self.pending_remove_list()?;

        if pending_removal_list.is_empty() {
            tracing::debug!(
                group_id = %self.group_id,
                "No pending removals to clean up"
            );
            // Clear the pending leave request status
            self.context
                .db()
                .set_group_has_pending_leave_request_status(&self.group_id, Some(false))?;
            return Ok(());
        }

        // Get current group members
        let current_members = self.members().await?;
        let current_member_ids: Vec<String> = current_members
            .iter()
            .map(|member| member.inbox_id.clone())
            .collect();

        // Calculate removed members: users in pending list but not in current group
        let removed_members: Vec<String> = pending_removal_list
            .iter()
            .filter(|pending_user| !current_member_ids.contains(pending_user))
            .cloned()
            .collect();

        if !removed_members.is_empty() {
            tracing::info!(
                group_id = %self.group_id,
                removed_count = removed_members.len(),
                removed_members = ?removed_members,
                "Removing members from pending removal list - they are no longer in the group"
            );

            // Remove all users who are no longer in the group from pending list
            self.context
                .db()
                .delete_pending_remove_users(&self.group_id, removed_members)?;
        }

        // After cleanup, check if there are any pending removals left
        let remaining_pending_list = self.pending_remove_list()?;
        if remaining_pending_list.is_empty() {
            // Clear the pending leave request status if no pending removals remain
            self.context
                .db()
                .set_group_has_pending_leave_request_status(&self.group_id, Some(false))?;
        }

        tracing::info!(
            group_id = %self.group_id,
            remaining_pending = remaining_pending_list.len(),
            "Finished cleaning up pending removal list"
        );

        Ok(())
    }

    pub async fn leave_group(&self) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        // Check if user is a member
        let is_member = self.is_member().await?;
        if !is_member {
            return Err(GroupLeaveValidationError::NotAGroupMember.into());
        }

        //check member size
        let members = self.members().await?;

        // check if the group has other members
        if members.len() == 1 {
            return Err(GroupLeaveValidationError::SingleMemberLeaveRejected.into());
        }

        // check if the conversation is not a DM
        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(GroupLeaveValidationError::DmLeaveForbidden.into());
        }

        let is_super_admin = self.is_super_admin(self.context.inbox_id().to_string())?;

        // super-admin cannot leave a group; must be demoted first
        // since SuperAdmins can't remove other SuperAdmins they need to be demoted first
        if is_super_admin {
            return Err(GroupLeaveValidationError::SuperAdminLeaveForbidden.into());
        }

        if !self.is_in_pending_remove(self.context.inbox_id())? {
            let content = LeaveRequestCodec::encode(LeaveRequest {
                authenticated_note: None,
            })?;
            self.send_message(
                &encoded_content_to_bytes(content),
                SendMessageOpts::default(),
            )
            .await?;
        };
        Ok(())
    }

    /// Checks if the current user is a member of the group.
    /// Returns true if the user is a member, false otherwise.
    #[tracing::instrument(level = "debug", skip(self))]
    async fn is_member(&self) -> Result<bool, GroupError> {
        let members = self.members().await?;
        Ok(members
            .iter()
            .any(|m| m.inbox_id == self.context.inbox_id()))
    }
}
