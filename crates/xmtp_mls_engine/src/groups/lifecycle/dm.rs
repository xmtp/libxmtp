//! DM lifecycle: creating, finding and storing DMs.

use super::*;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// Create a DM between this installation's inbox and
    /// `dm_target_inbox_id` and store it, without adding the peer.
    pub fn create_dm_and_insert(
        context: &Context,
        membership_state: GroupMembershipState,
        dm_target_inbox_id: InboxId,
        opts: GroupMetadataOptions,
        existing_group_id: Option<&[u8]>,
    ) -> Result<Self, GroupError> {
        Self::create_dm_with_members(
            context,
            membership_state,
            DmMembers {
                member_one_inbox_id: context.inbox_id().to_string(),
                member_two_inbox_id: dm_target_inbox_id,
            },
            opts,
            existing_group_id,
        )
    }

    /// The active DM between this installation's inbox and `inbox_id`, or
    /// `None`. A stored DM that lacks the peer gets the peer first, and any
    /// failure of that add is returned.
    pub async fn find_active_dm<E>(
        context: &Context,
        inbox_id: InboxIdRef<'_>,
    ) -> Result<Option<Self>, E>
    where
        E: From<xmtp_db::ConnectionError> + From<GroupError>,
    {
        let stored = context.db().find_active_dm_group(&DmMembers {
            member_one_inbox_id: context.inbox_id(),
            member_two_inbox_id: inbox_id,
        })?;
        let Some(stored) = stored else {
            return Ok(None);
        };
        let group = Self::new(
            context.clone(),
            stored.id,
            stored.dm_id,
            stored.conversation_type,
            stored.created_at_ns,
        );
        // implements: DMS-020
        group.add_missing_dm_peer().await?;
        Ok(Some(group))
    }

    /// Create an allowed DM with `peer_inbox_id`, with the disappearing-message
    /// settings of `opts`, and store it without adding the peer.
    pub fn create_allowed_dm(
        context: &Context,
        peer_inbox_id: InboxId,
        opts: Option<DMMetadataOptions>,
    ) -> Result<Self, GroupError> {
        Self::create_dm_and_insert(
            context,
            GroupMembershipState::Allowed,
            peer_inbox_id,
            GroupMetadataOptions {
                message_disappearing_settings: opts
                    .unwrap_or_default()
                    .message_disappearing_settings,
                ..Default::default()
            },
            None,
        )
    }

    /// Store an allowed DM with `peer_inbox_id` without adding the peer: the
    /// state a creation leaves when it stops between the insert and the add.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn create_dm_without_peer(
        context: &Context,
        peer_inbox_id: InboxId,
    ) -> Result<Self, GroupError> {
        Self::create_dm_and_insert(
            context,
            GroupMembershipState::Allowed,
            peer_inbox_id,
            GroupMetadataOptions::default(),
            None,
        )
    }

    fn create_dm_with_members(
        context: &Context,
        membership_state: GroupMembershipState,
        dm_members: DmMembers<InboxId>,
        opts: GroupMetadataOptions,
        existing_group_id: Option<&[u8]>,
    ) -> Result<Self, GroupError> {
        let commit_log_enabled = context.server_configuration().commit_log_enabled();
        let (group_id, root) = NewGroupId::draw(existing_group_id)?;
        let kind = if membership_state == GroupMembershipState::Restored {
            InitialGroupKind::RestoredDm {
                member_one_inbox_id: &dm_members.member_one_inbox_id,
                member_two_inbox_id: &dm_members.member_two_inbox_id,
            }
        } else {
            InitialGroupKind::Dm {
                target_inbox_id: &dm_members.member_two_inbox_id,
            }
        };
        let dictionary = initial_dictionary(
            kind,
            &PolicySet::new_dm()
                .to_proto()
                .map_err(group_permissions::GroupMutablePermissionsError::from)
                .map_err(MetadataPermissionsError::from)?,
            &opts,
            context.inbox_id(),
            root.as_ref(),
            &context
                .server_configuration()
                .configuration()
                .application_components,
        )
        .map_err(app_data::migration::BootstrapSynthesisError::from)?;
        // The dictionary holds the root now; wipe this copy.
        drop(root);
        let group_config = build_group_config(dictionary)?;

        let stored_group = if membership_state == GroupMembershipState::Restored {
            state_write(context.mls_storage(), |tx| {
                Ok::<_, GroupError>(Continue(Self::insert_dm_row(
                    context,
                    tx,
                    membership_state,
                    &dm_members,
                    &opts,
                    group_id,
                    &group_config,
                    commit_log_enabled,
                )?))
            })?
            .into_continued()
            .0
        } else {
            xmtp_mls_core::state_tx::state_write_with_events(
                context.mls_storage(),
                context.events(),
                |tx, events| {
                    let (stored_group, created, consent) = Self::insert_dm_row(
                        context,
                        tx,
                        membership_state,
                        &dm_members,
                        &opts,
                        group_id,
                        &group_config,
                        commit_log_enabled,
                    )?;
                    if created {
                        let storage = tx.storage();
                        let changes: Vec<_> = consent.iter().cloned().collect();
                        emit_consent_changes(context, events, &storage.db(), &changes)?;
                        if existing_group_id.is_none() {
                            context.task_channels().mark_notification_changed();
                            events.emit_with_context(
                                Some(xmtp_events::ClientEvent::ConversationJoined(
                                    xmtp_events::ConversationJoined {
                                        group_id: stored_group.id.to_vec(),
                                        conversation_type: xmtp_events::ConversationType::Dm,
                                        origin: xmtp_events::JoinOrigin::Created,
                                        adder_inbox_id: None,
                                    },
                                )),
                                Some(xmtp_mls_core::events::InternalEvent::GroupJoined {
                                    group_id: stored_group.id,
                                    is_sync: false,
                                }),
                                dm_event_context(stored_group.dm_id.as_deref()),
                            );
                        }
                    }
                    Ok::<_, GroupError>(Continue((stored_group, created, consent)))
                },
            )?
            .into_continued()
            .0
        };
        let new_group = Self::new_from_arc(
            context.clone(),
            stored_group.id,
            stored_group.dm_id,
            ConversationType::Dm,
            stored_group.created_at_ns,
        );
        Ok(new_group)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the row stores the DM creation parameters"
    )]
    pub(super) fn insert_dm_row(
        context: &Context,
        tx: &mut xmtp_mls_core::state_tx::StateTx<'_, impl TransactionalKeyStore>,
        membership_state: GroupMembershipState,
        dm_members: &DmMembers<InboxId>,
        opts: &GroupMetadataOptions,
        id: NewGroupId,
        group_config: &MlsGroupCreateConfig,
        commit_log_enabled: bool,
    ) -> Result<(StoredGroup, bool, Option<ConsentChange>), GroupError> {
        let storage = tx.storage();
        let db = storage.db();
        if let NewGroupId::Placeholder(group_id) = id
            && let Some(existing) = db.find_group(&group_id)?
        {
            return Ok((existing, false, None));
        }
        let mls_group =
            Self::create_mls_group(context, &storage, id, group_config, commit_log_enabled)?;
        let group_id: GroupId = mls_group.group_id().try_into()?;
        let stored_group = StoredGroup::builder()
            .id(group_id)
            .created_at_ns(now_ns())
            .membership_state(membership_state)
            .added_by_inbox_id(context.inbox_id().to_string())
            .message_disappear_from_ns(
                opts.message_disappearing_settings
                    .as_ref()
                    .map(|m| m.from_ns),
            )
            .message_disappear_in_ns(opts.message_disappearing_settings.as_ref().map(|m| m.in_ns))
            .dm_id(Some(dm_members.to_string()))
            .commit_log_public_key(id.commit_log_id())
            .build()?;
        stored_group.store(&db)?;
        // A DM's consent is its peer's inbox record.
        let own = context.inbox_id();
        let consent = match ConsentKey::of_group(&stored_group, own) {
            _ if membership_state == GroupMembershipState::Restored => {
                fold_into_inbox(&db, own, &stored_group)?
            }
            // implements: CONS-056
            Some(key) => {
                let created = ConsentWrite::Decide(Decision::Created);
                write_consent(&db, &key, created, Owed::Mark)?
            }
            None => None,
        };
        Ok((stored_group, true, consent))
    }
}
