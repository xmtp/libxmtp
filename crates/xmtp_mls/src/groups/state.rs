//! Admin lists, consent, epoch state, and group context.

use super::*;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// Updates the admin list of the group and syncs the changes to the network.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn update_admin_list(
        &self,
        action_type: UpdateAdminListType,
        inbox_id: String,
    ) -> Result<(), GroupError> {
        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_action_type = match action_type {
            UpdateAdminListType::Add => AdminListActionType::Add,
            UpdateAdminListType::Remove => AdminListActionType::Remove,
            UpdateAdminListType::AddSuper => AdminListActionType::AddSuper,
            UpdateAdminListType::RemoveSuper => AdminListActionType::RemoveSuper,
        };
        let intent_data: Vec<u8> =
            UpdateAdminListIntentData::new(intent_action_type, inbox_id).into();
        let intent = QueueIntent::update_admin_list()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Find the `inbox_id` of the group member who added the member to the group
    pub fn added_by_inbox_id(&self) -> Result<String, GroupError> {
        let conn = self.context.db();
        let group = conn
            .find_group(&self.group_id)?
            .ok_or(NotFound::GroupById(self.group_id))?;
        Ok(group.added_by_inbox_id)
    }

    /// Find the `consent_state` of the group
    pub fn consent_state(&self) -> Result<ConsentState, GroupError> {
        let conn = self.context.db();
        let record =
            conn.get_consent_record(hex::encode(self.group_id), ConsentType::ConversationId)?;

        match record {
            Some(rec) => Ok(rec.state),
            None => Ok(ConsentState::Unknown),
        }
    }

    // Returns new consent records. Does not broadcast changes.
    pub fn quietly_update_consent_state(
        &self,
        state: ConsentState,
        db: &impl DbQuery,
    ) -> Result<Vec<StoredConsentRecord>, GroupError> {
        let consent_record = StoredConsentRecord::new(
            ConsentType::ConversationId,
            state,
            hex::encode(self.group_id),
        );

        Ok(db.insert_or_replace_consent_records(std::slice::from_ref(&consent_record))?)
    }

    #[tracing::instrument(skip_all, level = "trace")]
    pub fn update_consent_state(&self, state: ConsentState) -> Result<(), GroupError> {
        let db = self.context.db();
        let new_records: Vec<PreferenceUpdate> = self
            .quietly_update_consent_state(state, &db)?
            .into_iter()
            .map(PreferenceUpdate::Consent)
            .collect();

        if !new_records.is_empty() {
            self.context.task_channels().wake_notifications();
            // Dispatch an update event so it can be synced across devices
            let _ = self
                .context
                .worker_events()
                .send(SyncWorkerEvent::SyncPreferences(new_records.clone()));
            // Broadcast the changes
            let _ = self
                .context
                .local_events()
                .send(LocalEvents::PreferencesChanged(new_records));
        }

        Ok(())
    }

    /// Get the current epoch number of the group.
    pub async fn epoch(&self) -> Result<u64, GroupError> {
        self.with_group_snapshot(|mls_group| Ok(mls_group.epoch().as_u64()))
    }

    /// Get the encryption state of the current epoch. Should match for all installations
    /// in the same epoch.
    #[cfg(test)]
    pub(crate) async fn epoch_authenticator(&self) -> Result<Vec<u8>, GroupError> {
        self.with_group_snapshot(|mls_group| {
            Ok(mls_group.epoch_authenticator().as_slice().to_vec())
        })
    }

    pub async fn cursor(&self) -> Result<Cursor, GroupError> {
        let db = self.context.db();
        let msgs = db.get_last_cursor(self.group_id, EntityKind::ApplicationMessage)?;
        Ok(msgs)
    }

    pub async fn local_commit_log(&self) -> Result<Vec<LocalCommitLog>, GroupError> {
        Ok(self.context.db().get_group_logs(&self.group_id)?)
    }

    pub async fn remote_commit_log(&self) -> Result<Vec<RemoteCommitLog>, GroupError> {
        Ok(self.context.db().get_remote_commit_log_after_cursor(
            &self.group_id,
            0,
            RemoteCommitLogOrder::AscendingByRowid,
        )?)
    }

    pub async fn debug_info(&self) -> Result<ConversationDebugInfo, GroupError> {
        let epoch = self.epoch().await?;
        let cursor = self.cursor().await?;
        let commit_log = self.local_commit_log().await?;
        let remote_commit_log = self.remote_commit_log().await?;
        let db = self.context.db();

        let stored_group = match db.find_group(&self.group_id)? {
            Some(group) => group,
            None => {
                return Err(GroupError::NotFound(NotFound::GroupById(self.group_id)));
            }
        };

        Ok(ConversationDebugInfo {
            epoch,
            maybe_forked: stored_group.maybe_forked,
            fork_details: stored_group.fork_details,
            is_commit_log_forked: stored_group.is_commit_log_forked,
            local_commit_log: format!("{:?}", commit_log),
            remote_commit_log: format!("{:?}", remote_commit_log),
            cursor: vec![cursor],
        })
    }

    /// Update this installation's leaf key in the group by creating a key update commit
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn key_update(&self) -> Result<(), GroupError> {
        let intent = QueueIntent::key_update().queue(self)?;
        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Checks if the current user is active in the group.
    ///
    /// If the current user has been kicked out of the group, `is_active` will return `false`
    #[tracing::instrument(skip_all, level = "trace")]
    pub fn is_active(&self) -> Result<bool, GroupError> {
        // Restored groups that are not yet added are inactive
        let Some(stored_group) = self.context.db().find_group(&self.group_id)? else {
            return Err(GroupError::NotFound(NotFound::GroupById(self.group_id)));
        };
        if matches!(
            stored_group.membership_state,
            GroupMembershipState::Restored
        ) {
            return Ok(false);
        }

        self.with_group_snapshot(|mls_group| Ok(mls_group.is_active()))
    }

    /// Returns the membership state of the current user in this group.
    #[tracing::instrument(skip_all, level = "trace")]
    pub fn membership_state(&self) -> Result<GroupMembershipState, GroupError> {
        let stored_group = self
            .context
            .db()
            .find_group(&self.group_id)?
            .ok_or_else(|| GroupError::NotFound(NotFound::GroupById(self.group_id)))?;
        Ok(stored_group.membership_state)
    }

    /// Get the `GroupMetadata` of the group.
    ///
    /// On migrated groups the legacy immutable-metadata extension has
    /// been removed; synthesize from dict (CONVERSATION_TYPE,
    /// CREATOR_INBOX_ID, DM_MEMBERS, ONESHOT_MESSAGE). On unmigrated
    /// groups, the legacy extension is authoritative.
    ///
    /// Migrated-but-no-seeds is treated as a hard error rather than
    /// falling through to the legacy extension — the bootstrap commit
    /// strips the legacy `GroupContextExtension`, so falling through
    /// would surface an unrelated `MissingExtension` from the legacy
    /// path. Returning `MissingExtension` directly here keeps the
    /// failure shape callers already handle while making the
    /// "incomplete migration" condition explicit at the originating
    /// site.
    pub async fn metadata(&self) -> Result<GroupMetadata, GroupError> {
        self.with_group_snapshot(|mls_group| {
            if self::app_data::is_migrated_group(mls_group) {
                let seed =
                    self::app_data::component_source::read_group_metadata_from_dict(mls_group)
                        .map_err(MetadataPermissionsError::from)?
                        .ok_or_else(|| {
                            MetadataPermissionsError::from(GroupMetadataError::MissingExtension)
                        })?;
                use xmtp_proto::xmtp::mls::message_contents::GroupMetadataV1 as GroupMetadataProto;
                // `creator_account_address` has been `""` on the
                // legacy write path since long before this migration
                // (see the `TODO: remove from proto` note in
                // `xmtp_mls_common::group_metadata`). The field is
                // effectively dead — no consumer reads it — so the
                // migrated synthesis keeps it empty to match legacy
                // bytes exactly.
                let proto = GroupMetadataProto {
                    conversation_type: seed.conversation_type,
                    creator_inbox_id: seed.creator_inbox_id,
                    creator_account_address: String::new(),
                    dm_members: seed.dm_members,
                    oneshot_message: seed.oneshot,
                };
                return Ok(GroupMetadata::try_from(proto).map_err(MetadataPermissionsError::from)?);
            }
            extract_group_metadata(mls_group.extensions())
                .map_err(MetadataPermissionsError::from)
                .map_err(Into::into)
        })
    }

    /// Read the group's `GroupContext` from storage — a single KV round-trip,
    /// no ratchet tree, no secrets, no commit lock. All group metadata lives in
    /// the context extensions, so metadata reads go through this rather than a
    /// full `OpenMlsGroup::load`. The context key is written atomically on
    /// commit, so a single-key read is metadata-consistent.
    ///
    /// (The pre-refactor sync `load_mls_group_with_lock` used only an *advisory*
    /// lock for these reads — a failed `get_lock_sync` was ignored and the read
    /// proceeded anyway — so dropping it changes nothing for reads.)
    pub(crate) fn load_group_context(&self) -> Result<openmls::group::GroupContext, GroupError> {
        use openmls_traits::storage::StorageProvider as _;
        self.context
            .mls_storage()
            .group_context::<_, openmls::group::GroupContext>(&self.group_id.to_openmls())
            .map_err(GroupError::from)?
            .ok_or_else(|| GroupError::from(StorageError::from(NotFound::GroupById(self.group_id))))
    }

    /// Get the `GroupMutableMetadata` of the group.
    ///
    /// Post-migration (dict contains `COMPONENT_REGISTRY` — see
    /// [`self::app_data::is_migrated_group`]) the legacy GMM extension
    /// is gone; we start with an empty base and
    /// `merge_app_data_into_mutable_metadata` populates every field
    /// from the AppData dict. Pre-migration we read the legacy GMM
    /// extension authoritatively. The overlay helper itself also
    /// checks the migration marker (defense in depth), so a stray
    /// dict entry on a pre-bootstrap group can't silently shadow
    /// legacy values.
    ///
    /// Intentionally distinct from `proposals_enabled`: a group can
    /// have `proposals_enabled == true` but not yet have completed
    /// its bootstrap commit, during which window the legacy GMM is
    /// still authoritative.
    pub fn mutable_metadata(&self) -> Result<GroupMutableMetadata, GroupError> {
        use self::app_data::component_source::ComponentSourceError;
        let ctx = self.load_group_context()?;
        self::app_data::component_source::extract_group_mutable_metadata_capability_aware_from_extensions(
            ctx.extensions(),
        )
        .map_err(|e| match e {
            // Inner `GroupMutableMetadataError` originates from the legacy
            // `TryFrom<&Extensions>` path on unmigrated groups; the
            // `From<ComponentSourceError>` impl preserves it verbatim so binding
            // consumers that pattern-match on `MetadataPermissionsError::Mutable`
            // keep lighting up on `MissingExtension`.
            ComponentSourceError::GroupMutableMetadata(inner) => {
                GroupError::MetadataPermissionsError(MetadataPermissionsError::Mutable(inner))
            }
            other => GroupError::MetadataPermissionsError(
                MetadataPermissionsError::ComponentSource(other),
            ),
        })
    }

    /// Pre-L implementation of [`Self::mutable_metadata`]: a full
    /// `OpenMlsGroup::load`. Retained only as the baseline the
    /// read-amplification benchmark measures the context-read path against.
    #[cfg(test)]
    pub(crate) fn mutable_metadata_via_full_load(
        &self,
    ) -> Result<GroupMutableMetadata, GroupError> {
        use self::app_data::component_source::ComponentSourceError;
        self.load_mls_group_with_lock(self.context.mls_storage(), |mls_group| {
            self::app_data::component_source::extract_group_mutable_metadata_capability_aware(
                &mls_group,
            )
            .map_err(|e| match e {
                ComponentSourceError::GroupMutableMetadata(inner) => {
                    GroupError::MetadataPermissionsError(MetadataPermissionsError::Mutable(inner))
                }
                other => GroupError::MetadataPermissionsError(
                    MetadataPermissionsError::ComponentSource(other),
                ),
            })
        })
    }

    pub fn permissions(&self) -> Result<GroupMutablePermissions, GroupError> {
        let ctx = self.load_group_context()?;
        let permissions: GroupMutablePermissions = ctx
            .extensions()
            .try_into()
            .map_err(MetadataPermissionsError::from)?;
        Ok(permissions)
    }

    /// Capability-aware single-component read.
    ///
    /// Reads the group's `GroupContext` (via [`Self::load_group_context`]),
    /// then uses the [`self::app_data::typed_facade::MlsGroupAppData`] facade to
    /// read exactly one [`Component`](xmtp_mls_common::app_data::typed::Component)
    /// out of its extensions — avoiding both the full `OpenMlsGroup::load` and
    /// the full `GroupMutableMetadata` composite parse that a naive read would
    /// run on every call.
    ///
    /// Returns `Ok(None)` when the component has no stored value
    /// (legacy GMM attribute missing on unmigrated groups, or dict slot
    /// absent on migrated groups).
    ///
    /// Preserves the pre-refactor `GroupError::MetadataPermissionsError(...)`
    /// shape that `self.mutable_metadata()` produced. For the corrupted-
    /// legacy-GMM case the inner `GroupMutableMetadataError` is peeled
    /// out of `ComponentSourceError::GroupMutableMetadata(...)` and
    /// surfaced as `MetadataPermissionsError::Mutable(inner)`, matching
    /// what binding consumers used to see. Other `ComponentSourceError`
    /// variants (TLS codec, set/map apply failures) surface as
    /// `MetadataPermissionsError::ComponentSource(other)`.
    pub(in crate::groups) fn read_single_component<C>(&self) -> Result<Option<C::Value>, GroupError>
    where
        C: xmtp_mls_common::app_data::typed::Component,
    {
        use self::app_data::component_source::ComponentSourceError;
        let ctx = self.load_group_context()?;
        let facade = self::app_data::typed_facade::MlsGroupAppData::new(ctx.extensions());
        facade.get::<C>().map_err(|e| match e {
            ComponentSourceError::GroupMutableMetadata(inner) => {
                GroupError::MetadataPermissionsError(MetadataPermissionsError::Mutable(inner))
            }
            other => GroupError::MetadataPermissionsError(
                MetadataPermissionsError::ComponentSource(other),
            ),
        })
    }

    /// Fetches the message disappearing settings for a given group ID.
    ///
    /// Returns `Some(MessageDisappearingSettings)` if the group exists and has valid settings,
    /// `None` if the group or settings are missing, or `Err(ClientError)` on a database error.
    pub fn disappearing_settings(&self) -> Result<Option<MessageDisappearingSettings>, GroupError> {
        let conn = self.context.db();
        let stored_group: Option<StoredGroup> = conn.fetch(&self.group_id)?;

        let settings = stored_group.and_then(|group| {
            let from_ns = group.message_disappear_from_ns?;
            let in_ns = group.message_disappear_in_ns?;

            Some(MessageDisappearingSettings { from_ns, in_ns })
        });

        Ok(settings)
    }

    /// Find all the duplicate dms for this group
    pub fn find_duplicate_dms(&self) -> Result<Vec<MlsGroup<Context>>, ClientError> {
        let duplicates = self.context.db().other_dms(&self.group_id)?;

        let mls_groups = duplicates
            .into_iter()
            .map(|g| {
                MlsGroup::new(
                    self.context.clone(),
                    g.id,
                    g.dm_id,
                    g.conversation_type,
                    g.created_at_ns,
                )
            })
            .collect();

        Ok(mls_groups)
    }

    /// Used for testing that dm group validation works as expected.
    ///
    /// See the `test_validate_dm_group` test function for more details.
    #[cfg(test)]
    pub fn create_test_dm_group(
        context: Context,
        dm_target_inbox_id: InboxId,
        custom_protected_metadata: Option<Extension>,
        custom_mutable_metadata: Option<Extension>,
        custom_group_membership: Option<Extension>,
        custom_mutable_permissions: Option<PolicySet>,
        opts: Option<DMMetadataOptions>,
    ) -> Result<Self, GroupError> {
        let provider = context.mls_provider();

        let protected_metadata = custom_protected_metadata.unwrap_or_else(|| {
            build_dm_protected_metadata_extension(context.inbox_id(), dm_target_inbox_id.clone())
                .unwrap()
        });
        let mutable_metadata = custom_mutable_metadata.unwrap_or_else(|| {
            build_dm_mutable_metadata_extension_default(
                context.inbox_id(),
                &dm_target_inbox_id,
                opts.unwrap_or_default(),
            )
            .unwrap()
        });
        let group_membership = custom_group_membership
            .unwrap_or_else(|| build_starting_group_membership_extension(context.inbox_id(), 0));
        let mutable_permissions = custom_mutable_permissions.unwrap_or_else(PolicySet::new_dm);
        let mutable_permission_extension =
            build_mutable_permissions_extension(mutable_permissions)?;

        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permission_extension,
        )?;

        let mls_group =
            OpenMlsGroup::from_creation_logged(&provider, context.identity(), &group_config)?;
        let group_id: GroupId = mls_group.group_id().try_into()?;
        let stored_group = StoredGroup::builder()
            .id(group_id)
            .created_at_ns(now_ns())
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id(context.inbox_id().to_string())
            .dm_id(Some(
                DmMembers {
                    member_one_inbox_id: context.inbox_id().to_string(),
                    member_two_inbox_id: dm_target_inbox_id,
                }
                .to_string(),
            ))
            .build()?;

        stored_group.store(&context.db())?;
        Ok(Self::new_from_arc(
            context,
            group_id,
            stored_group.dm_id.clone(),
            ConversationType::Dm,
            stored_group.created_at_ns,
        ))
    }
}
