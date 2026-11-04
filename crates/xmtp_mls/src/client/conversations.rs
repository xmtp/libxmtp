//! Client conversations: creating and finding groups and DMs, messages, and the
//! conversation list.

use super::*;

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    /// Create a new group with the default settings
    /// Applies a custom [`PolicySet`] to the group if one is specified
    pub fn create_group(
        &self,
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        self.ensure_identity_ready::<ClientError>()?;

        let group: MlsGroup<Context> = MlsGroup::create_and_insert(
            self.context.clone(),
            ConversationType::Group,
            permissions_policy_set.unwrap_or_default(),
            opts.unwrap_or_default(),
            None,
        )?;

        log_event!(
            Event::CreatedGroup,
            self.context.installation_id(),
            group_id = group.group_id
        );

        Ok(group)
    }

    /// Create a group with an initial set of members added. Every account
    /// resolves before the group is stored, as do member identities and key
    /// packages. An account with no inbox fails with [`GroupError::AddressNotFound`].
    pub async fn create_group_with_identifiers(
        &self,
        account_identifiers: &[Identifier],
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        self.create_group_with_targets(
            &account_targets(account_identifiers),
            permissions_policy_set,
            opts,
        )
        .await
    }

    /// Returns the stored group if its first add is still pending when the wait
    /// ends. A later sync completes the add.
    #[tracing::instrument(level = "debug", skip_all, fields(size = inbox_ids.len()))]
    pub async fn create_group_with_members(
        &self,
        inbox_ids: &[impl AsIdRef],
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        tracing::info!("creating group");
        self.ensure_identity_ready::<ClientError>()?;
        let (group, first_add) =
            MlsGroup::create_with_first_add(&self.context, inbox_ids, permissions_policy_set, opts)
                .await?;
        log_event!(
            Event::CreatedGroup,
            self.context.installation_id(),
            group_id = group.group_id
        );
        group.sync_first_add(first_add).await?;

        Ok(group)
    }

    /// Create a new Direct Message with the default settings
    #[tracing::instrument(level = "debug", skip_all)]
    pub(super) async fn create_dm_by_inbox_id(
        &self,
        target_inbox_id: InboxId,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        let group: MlsGroup<Context> =
            MlsGroup::create_allowed_dm(&self.context, target_inbox_id.clone(), opts)?;

        log_event!(
            Event::CreatedDM,
            self.context.installation_id(),
            group_id = group.group_id,
            target_inbox = target_inbox_id
        );
        group.add_members(&[target_inbox_id]).await?;

        Ok(group)
    }

    /// Find or create a Direct Message with the default settings
    pub async fn find_or_create_dm_by_identity(
        &self,
        target_identity: Identifier,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        self.ensure_identity_ready::<ClientError>()?;
        tracing::info!("finding or creating dm with address: {target_identity}");
        let inbox_id = match self
            .find_inbox_id_from_identifier(target_identity.clone())
            .await?
        {
            Some(id) => id,
            None => {
                return Err(NotFound::InboxIdForAddress(target_identity.to_string()).into());
            }
        };

        self.find_or_create_dm(inbox_id, opts).await
    }

    /// Find or create a Direct Message by inbox_id with the default settings.
    /// A stored DM whose creation stopped before the peer was added gets the
    /// peer first, and any failure of that add is returned, as a new DM's
    /// creation returns its add's failure.
    pub async fn find_or_create_dm(
        &self,
        inbox_id: impl AsIdRef,
        opts: Option<DMMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        self.ensure_identity_ready::<ClientError>()?;
        let inbox_id = inbox_id.as_ref();
        tracing::info!("finding or creating dm with inbox_id: {}", inbox_id);
        if let Some(group) =
            MlsGroup::find_active_dm::<ClientError>(&self.context, inbox_id).await?
        {
            return Ok(group);
        }
        self.create_dm_by_inbox_id(inbox_id.to_string(), opts).await
    }

    /// Look up a group by its ID
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn group(&self, group_id: &GroupId) -> Result<MlsGroup<Context>, ClientError> {
        MlsGroup::load_by_id(&self.context, group_id).map_err(Into::into)
    }

    /// Look up a group by its ID while stitching DMs
    ///
    /// Returns a [`MlsGroup`] if the group exists, or an error if it does not
    ///
    pub fn stitched_group(&self, group_id: &GroupId) -> Result<MlsGroup<Context>, ClientError> {
        let conn = self.context.db();
        let stored_group = conn.fetch_stitched(group_id)?;
        stored_group
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .ok_or(NotFound::GroupById(*group_id))
            .map_err(Into::into)
    }

    /// Find all the duplicate dms for this group
    pub fn find_duplicate_dms_for_group(
        &self,
        group_id: &GroupId,
    ) -> Result<Vec<MlsGroup<Context>>, ClientError> {
        let (group, _) = MlsGroup::new_cached(self.context.clone(), group_id)?;
        Ok(group.find_duplicate_dms()?)
    }

    /// Fetches the message disappearing settings for a given group ID.
    ///
    /// Returns `Some(MessageDisappearingSettings)` if the group exists and has valid settings,
    /// `None` if the group or settings are missing, or `Err(ClientError)` on a database error.
    pub fn group_disappearing_settings(
        &self,
        group_id: &GroupId,
    ) -> Result<Option<MessageDisappearingSettings>, ClientError> {
        let (group, _) = MlsGroup::new_cached(self.context.clone(), group_id)?;
        Ok(group.disappearing_settings()?)
    }

    /**
     * Look up a DM group by the target's inbox_id.
     *
     * Returns a [`MlsGroup`] if the group exists, or an error if it does not
     */
    pub fn dm_group_from_target_inbox(
        &self,
        target_inbox_id: String,
    ) -> Result<MlsGroup<Context>, ClientError> {
        let conn = self.context.db();

        let group = conn
            .find_active_dm_group(&DmMembers {
                member_one_inbox_id: self.inbox_id(),
                member_two_inbox_id: &target_inbox_id,
            })?
            .ok_or(NotFound::DmByInbox(target_inbox_id))?;
        Ok(MlsGroup::new(
            self.context.clone(),
            group.id,
            group.dm_id,
            group.conversation_type,
            group.created_at_ns,
        ))
    }

    /// Look up a message by its ID
    /// Returns a [`StoredGroupMessage`] if the message exists, or an error if it does not
    pub fn message(&self, message_id: Vec<u8>) -> Result<StoredGroupMessage, ClientError> {
        let conn = &mut self.context.db();
        // implements: META-051
        let message =
            conn.get_app_visible_group_message(&message_id, xmtp_common::time::now_ns())?;
        Ok(message.ok_or(NotFound::MessageById(message_id))?)
    }

    /// Look up a stored message and its current conversation.
    /// A message in a duplicate DM resolves to the stitched conversation.
    pub async fn message_with_group(
        &self,
        id: &[u8],
    ) -> Result<Option<(StoredGroupMessage, MlsGroup<Context>)>, ClientError> {
        let message = match self.message(id.to_vec()) {
            Ok(message) => message,
            Err(ClientError::Storage(StorageError::NotFound(NotFound::MessageById(_)))) => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let group = self.stitched_group(&message.group_id)?;
        Ok(Some((message, group)))
    }

    /// Look up and enrich a message by its ID, returning a [`DecodedMessage`]
    /// Returns an error if the message is not found or if it cannot be decoded/enriched
    #[xmtp_common::mls_span]
    pub fn message_v2(&self, message_id: Vec<u8>) -> Result<DecodedMessage, ClientError> {
        let conn = self.context.db();
        let message = conn
            .get_app_visible_group_message(&message_id, xmtp_common::time::now_ns())?
            .ok_or_else(|| NotFound::MessageById(message_id.clone()))?;

        let group_id = message.group_id;

        let enriched = enrich_messages(conn, &group_id, vec![message])?;

        // Since enrich_messages returns a Vec<DecodedMessage>, we can use .into_iter().next().ok_or(...) to take ownership without cloning.
        enriched
            .into_iter()
            .next()
            // In practice `enrich_messages` should always return an array of the same length as the input
            .ok_or(ClientError::MessageDecodeFailed)
    }

    /// Delete a message by its ID
    /// This method is idempotent and will not error if the message is not found
    /// Returns the number of messages deleted (0 or 1)
    pub fn delete_message(&self, message_id: Vec<u8>) -> Result<usize, ClientError> {
        Ok(crate::state_tx::state_write_with_events(
            self.context.mls_storage(),
            self.context.events(),
            |tx, events| {
                let storage = tx.storage();
                let db = storage.db();
                let message = db.get_group_message(&message_id)?;
                let deleted = db.delete_message_by_id(&message_id)?;
                if deleted > 0
                    && let Some(message) = message
                {
                    emit_deleted_messages(
                        events,
                        vec![message],
                        DeletionCause::DeletedLocally,
                        // A local removal of the row: no inbox sent a delete.
                        None,
                        &db,
                    )?;
                }
                Ok::<_, StorageError>(Continue(deleted))
            },
        )?
        .into_continued())
    }

    /// Query for groups with optional filters
    ///
    /// Filters:
    /// - allowed_states: only return groups with the given membership states
    /// - created_after_ns: only return groups created after the given timestamp (in nanoseconds)
    /// - created_before_ns: only return groups created before the given timestamp (in nanoseconds)
    /// - limit: only return the first `limit` groups
    pub fn find_groups(&self, args: GroupQueryArgs) -> Result<Vec<MlsGroup<Context>>, ClientError> {
        MlsGroup::find(&self.context, args).map_err(Into::into)
    }

    /// List one page of the app's conversations, in activity order unless
    /// `args` names another order. A page holds at most `args.limit`
    /// conversations, or [`CONVERSATION_LIST_DEFAULT_LIMIT`] when it names
    /// none. To read the next page, pass the last row's `listing_time_ns` as
    /// the order's before time and its group id as `before_id`.
    ///
    /// Internal callers that need every group use `find_groups` or
    /// `fetch_conversation_list`, which apply no default limit.
    // implements: CONS-046, CONS-049
    pub fn list_conversations(
        &self,
        mut args: GroupQueryArgs,
    ) -> Result<Vec<ConversationListItem<Context>>, ClientError> {
        if args.order_by.is_none() {
            args.order_by = Some(GroupQueryOrderBy::LastActivity);
        }
        let limit = args
            .limit
            .unwrap_or(i64::from(CONVERSATION_LIST_DEFAULT_LIMIT));
        if !(1..=i64::from(LOCAL_PAGE_MAX_LIMIT)).contains(&limit) {
            return Err(ClientError::InvalidListOptions(
                "limit must be at least 1 and at most the maximum page size",
            ));
        }
        args.limit = Some(limit);
        if args.before_id.is_some() && args.order_before_ns().is_none() {
            return Err(ClientError::InvalidListOptions(
                "before_id needs the before time of the list order",
            ));
        }
        // Requested sync groups bypass the page's limit, order and cursor, and
        // a sync group is not a conversation.
        if args.include_sync_groups || args.conversation_type.is_some_and(|kind| kind.is_virtual())
        {
            return Err(ClientError::InvalidListOptions(
                "a conversation listing cannot include sync or one-shot groups",
            ));
        }
        Ok(self
            .context
            .db()
            .fetch_conversation_list(args)?
            .into_iter()
            .map(
                |conversation_item: DbConversationListItem| ConversationListItem {
                    membership_state: conversation_item.membership_state,
                    added_by_inbox_id: conversation_item.added_by_inbox_id,
                    group: MlsGroup::new(
                        self.context.clone(),
                        conversation_item.id,
                        conversation_item.dm_id,
                        conversation_item.conversation_type,
                        conversation_item.created_at_ns,
                    ),
                    is_commit_log_forked: conversation_item.is_commit_log_forked,
                    listing_time_ns: conversation_item.listing_time_ns,
                },
            )
            .collect())
    }
}

impl<Context: XmtpSharedContext> Client<Context> {
    /// The active DM with `target`, or `None`. A target that no inbox holds
    /// has no DM.
    pub async fn find_dm_by_target(
        &self,
        target: &IdentityTarget,
    ) -> Result<Option<MlsGroup<Context>>, ClientError> {
        let Some(peer) =
            inbox_handles::resolve_verified_in(&self.context, std::slice::from_ref(target))
                .await?
                .pop()
                .flatten()
        else {
            return Ok(None);
        };
        let stored = self.context.db().find_active_dm_group(&DmMembers {
            member_one_inbox_id: self.inbox_id(),
            member_two_inbox_id: peer.as_str(),
        })?;
        stored.map(|stored| self.group(&stored.id)).transpose()
    }

    /// Find the DM with `target`, or create it. A handle resolves before the
    /// existing DM is looked up.
    pub async fn find_or_create_dm_by_target(
        &self,
        target: &IdentityTarget,
        opts: DMMetadataOptions,
    ) -> Result<MlsGroup<Context>, ClientError> {
        match target {
            IdentityTarget::Account(identifier) => {
                self.find_or_create_dm_by_identity(identifier.clone(), Some(opts))
                    .await
            }
            IdentityTarget::Handle(handle) => {
                let peer = self
                    .resolve_handles(std::slice::from_ref(handle))
                    .await?
                    .pop()
                    .flatten()
                    .ok_or(HandleError::NotFound)?;
                self.find_or_create_dm(peer, Some(opts)).await
            }
        }
    }

    /// Create a group with `targets` as its members, in one commit. Targets,
    /// member identities, and key packages resolve before the group is stored.
    /// A target with no inbox leaves no group: a handle fails with
    /// [`HandleError::NotFound`], an account with [`GroupError::AddressNotFound`].
    pub async fn create_group_with_targets(
        &self,
        targets: &[IdentityTarget],
        permissions_policy_set: Option<PolicySet>,
        opts: Option<GroupMetadataOptions>,
    ) -> Result<MlsGroup<Context>, ClientError> {
        let inbox_ids =
            crate::groups::targets::target_inboxes::<_, ClientError>(&self.context, targets)
                .await?;
        self.create_group_with_members(&inbox_ids, permissions_policy_set, opts)
            .await
    }
}
