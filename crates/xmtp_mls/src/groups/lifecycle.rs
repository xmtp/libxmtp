//! Construction, loading, proposal capability, and insertion.

use super::*;
use xmtp_db::TransactionalKeyStore;
use xmtp_events::EventWriter;
use xmtp_mls_common::app_data::creation::{InitialGroupKind, initial_dictionary};

/// Represents a group, which can contain anywhere from 1 to MAX_GROUP_SIZE inboxes.
///
/// This is a wrapper around OpenMLS's `MlsGroup` that handles our application-level configuration
/// and validations.
impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    // Creates a new group instance. Does not validate that the group exists in the DB
    pub fn new(
        context: Context,
        group_id: GroupId,
        dm_id: Option<String>,
        conversation_type: ConversationType,
        created_at_ns: i64,
    ) -> Self {
        Self::new_from_arc(
            context.clone(),
            group_id,
            dm_id,
            conversation_type,
            created_at_ns,
        )
    }

    /// Creates a new group instance from the database. Validate that the group exists in the DB before constructing
    /// the group.
    ///
    /// # Returns
    ///
    /// Returns the Group and the stored group information as a tuple.
    pub fn new_cached(
        context: Context,
        group_id: &GroupId,
    ) -> Result<(Self, StoredGroup), StorageError> {
        let conn = context.db();
        if let Some(group) = conn.find_group(group_id)? {
            Ok((
                Self::new_from_arc(
                    context,
                    *group_id,
                    group.dm_id.clone(),
                    group.conversation_type,
                    group.created_at_ns,
                ),
                group,
            ))
        } else {
            tracing::error!("group {} does not exist", hex::encode(group_id));
            Err(NotFound::GroupById(*group_id).into())
        }
    }

    pub(crate) fn new_from_arc(
        context: Context,
        group_id: GroupId,
        dm_id: Option<String>,
        conversation_type: ConversationType,
        created_at_ns: i64,
    ) -> Self {
        let mut mutexes = context.mutexes().clone();
        Self {
            group_id,
            dm_id,
            conversation_type,
            created_at_ns,
            mutex: mutexes.get_mutex(group_id),
            context: context.clone(),
            #[cfg(test)]
            mls_commit_lock: Arc::clone(context.mls_commit_lock()),
        }
    }

    /// Read a consistent MLS snapshot. Only immutable results leave the transaction.
    pub(crate) fn with_group_snapshot<R>(
        &self,
        operation: impl FnOnce(&OpenMlsGroup) -> Result<R, GroupError>,
    ) -> Result<R, GroupError> {
        state_write(self.context.mls_storage(), |tx| {
            tx.with_group(self.group_id, |group, _| operation(group))
                .map(Continue)
        })
        .map(TransactionOutcome::into_continued)
    }

    // Test fixtures can deliberately retain an MLS object to construct stale state.
    #[cfg(test)]
    #[tracing::instrument(level = "trace", skip_all)]
    pub(crate) fn load_mls_group_with_lock<F, R>(
        &self,
        storage: &impl XmtpMlsStorageProvider,
        operation: F,
    ) -> Result<R, GroupError>
    where
        F: Fn(OpenMlsGroup) -> Result<R, GroupError>,
    {
        // Get the group ID for locking
        let group_id = self.group_id;

        // Acquire the lock synchronously using blocking_lock
        let _lock = self.mls_commit_lock.get_lock_sync(group_id);
        // Load the MLS group
        let mls_group = OpenMlsGroup::load(storage, &self.group_id.to_openmls())
            .inspect_err(|e| tracing::error!("openmls error while loading group {e}"))
            .map_err(|_| NotFound::MlsGroup(self.group_id))?
            .ok_or(NotFound::MlsGroup(self.group_id))?;

        // Perform the operation with the MLS group
        operation(mls_group)
    }

    // Test fixtures can deliberately retain an MLS object across a network wait.
    #[cfg(test)]
    #[tracing::instrument(level = "trace", skip(operation))]
    pub(crate) async fn load_mls_group_with_lock_async<R, E>(
        &self,
        operation: impl AsyncFnOnce(OpenMlsGroup) -> Result<R, E>,
    ) -> Result<R, E>
    where
        E: From<crate::StorageError> + From<xmtp_db::sql_key_store::SqlKeyStoreError>,
    {
        let mls_storage = self.context.mls_storage();
        // Get the group ID for locking
        let group_id = self.group_id;

        // Acquire the lock asynchronously
        let _lock = self.mls_commit_lock.get_lock_async(group_id).await;

        // Load the MLS group
        let mls_group = OpenMlsGroup::load(mls_storage, &self.group_id.to_openmls())?
            .ok_or(StorageError::from(NotFound::GroupById(self.group_id)))?;

        // Perform the operation with the MLS group
        operation(mls_group).await
    }

    /// Check if all members in the group support the proposal-by-reference flow.
    ///
    /// This checks both:
    /// 1. Leaf node capabilities in the MLS group (via `check_extension_support`)
    /// 2. The latest published key packages for all member installations fetched
    ///    from the network, since leaf nodes may be stale (they aren't updated after
    ///    the first message is sent).
    ///
    /// Returns `true` if all members support proposals, `false` otherwise.
    pub async fn all_members_support_proposals(
        &self,
        mls_group: &OpenMlsGroup,
    ) -> Result<bool, GroupError> {
        let (supported, installation_ids) = self.proposal_support_snapshot(mls_group);
        self.published_members_support_proposals(supported, installation_ids)
            .await
    }

    fn proposal_support_snapshot(&self, group: &OpenMlsGroup) -> (bool, Vec<Vec<u8>>) {
        let supported = group
            .check_extension_support(&[ExtensionType::AppDataDictionary])
            .is_ok();
        let installation_ids = group
            .members()
            .map(|member| member.signature_key)
            .filter(|id| id.as_slice() != self.context.installation_id().as_slice())
            .collect();
        (supported, installation_ids)
    }

    async fn published_members_support_proposals(
        &self,
        supported: bool,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<bool, GroupError> {
        if supported || installation_ids.is_empty() {
            return Ok(true);
        }

        let store = crate::mls_store::MlsStore::new(self.context.clone());
        let key_packages = store
            .get_key_packages_for_installation_ids(installation_ids)
            .await?;

        for result in key_packages.values() {
            match result {
                Ok(verified_kp) => {
                    let capabilities = verified_kp.inner.leaf_node().capabilities();
                    if !capabilities
                        .extensions()
                        .contains(&ExtensionType::AppDataDictionary)
                    {
                        return Ok(false);
                    }
                }
                Err(_) => {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    /// Snapshot this group's membership capabilities: the extension types in
    /// the group context, plus the extension types each member installation
    /// advertises.
    ///
    /// This reports raw capability facts rather than answers — callers filter
    /// it to whatever question they care about. For the proposal
    /// (app-data-dictionary) migration specifically, a caller checks for
    /// [`MlsExtensionType::AppDataDictionary`] in `context_extensions` (already
    /// migrated?) and in each installation's `supported_extensions` (eligible /
    /// who is blocking?).
    ///
    /// Every installation's capabilities — the local one included — come from
    /// its *latest published* key package, not its in-group leaf node: a leaf
    /// is frozen when the installation joins and is never updated, so it would
    /// understate a client that upgraded afterward (the same reason
    /// [`Self::all_members_support_proposals`] falls back to key packages). An
    /// installation whose key package has not been published or fails
    /// verification is reported with `capabilities_known == false` and an empty
    /// extension list, so callers can distinguish "unknown" from "advertises
    /// nothing". Transient failures (network, auth, db) surface as an `Err`
    /// rather than masquerading as unknown capabilities.
    pub async fn membership_capabilities(&self) -> Result<GroupMembershipCapabilities, GroupError> {
        // Read the group context's extension types under lock. The member list
        // (below) is read in a separate lock acquisition, so this is a
        // best-effort snapshot rather than a single atomic view — fine for a
        // debug surface.
        let context_extensions = self.with_group_snapshot(|mls_group| {
            Ok::<_, GroupError>(
                mls_group
                    .extensions()
                    .iter()
                    .map(|ext| MlsExtensionType::from(ext.extension_type()))
                    .collect::<Vec<_>>(),
            )
        })?;

        let members = self.members().await?;
        let own_installation_id = self.context.installation_id();

        // Capabilities for every installation come from its latest published
        // key package. We intentionally include our own installation rather
        // than reading its in-group leaf, which is frozen at join and would
        // understate an upgraded local client.
        let query_ids: Vec<Vec<u8>> = members
            .iter()
            .flat_map(|member| member.installation_ids.iter().cloned())
            .collect();

        let extensions_by_installation = self.installation_extensions(query_ids).await?;

        let installation_capabilities = |installation_id: Vec<u8>| -> InstallationCapabilities {
            let is_own = installation_id.as_slice() == own_installation_id.as_slice();
            match extensions_by_installation.get(installation_id.as_slice()) {
                Some(extensions) => InstallationCapabilities {
                    installation_id,
                    is_own,
                    supported_extensions: extensions.clone(),
                    capabilities_known: true,
                },
                None => InstallationCapabilities {
                    installation_id,
                    is_own,
                    supported_extensions: Vec::new(),
                    capabilities_known: false,
                },
            }
        };

        let member_caps: Vec<InboxCapabilities> = members
            .into_iter()
            .map(|member| InboxCapabilities {
                inbox_id: member.inbox_id,
                installations: member
                    .installation_ids
                    .into_iter()
                    .map(installation_capabilities)
                    .collect(),
            })
            .collect();

        Ok(GroupMembershipCapabilities {
            context_extensions,
            members: member_caps,
        })
    }

    /// Fetch the latest published key package for each given installation and
    /// return the MLS extension types it advertises, keyed by installation id.
    ///
    /// An installation with no published key package (e.g. an old client — what
    /// this surface exists to flag) or whose key package fails verification is
    /// simply absent from the returned map; callers treat absence as
    /// "capabilities unknown". Transient/infrastructure failures (network,
    /// auth, db) are NOT swallowed — they propagate as `Err`, so a snapshot can
    /// tell "this installation has nothing published" apart from "we couldn't
    /// reach the server".
    ///
    async fn installation_extensions(
        &self,
        query_ids: Vec<Vec<u8>>,
    ) -> Result<HashMap<Vec<u8>, Vec<MlsExtensionType>>, GroupError> {
        if query_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let store = crate::mls_store::MlsStore::new(self.context.clone());

        let verified = store
            .get_key_packages_for_installation_ids(query_ids)
            .await?;

        Ok(verified
            .into_iter()
            .filter_map(|(id, result)| {
                let extensions = result
                    .ok()?
                    .inner
                    .leaf_node()
                    .capabilities()
                    .extensions()
                    .iter()
                    .copied()
                    .map(MlsExtensionType::from)
                    .collect();
                Some((id, extensions))
            })
            .collect())
    }

    /// Validate that key packages support the AppData dictionary
    /// group-context extension. A leaf that advertises
    /// `ExtensionType::AppDataDictionary` in its standard MLS
    /// `Capabilities` can join a migrated group and receive standalone
    /// `AppDataUpdate` proposals.
    pub fn validate_key_packages_support_proposals(
        &self,
        key_packages: &[openmls::key_packages::KeyPackage],
    ) -> Result<(), GroupError> {
        let extension_type = ExtensionType::AppDataDictionary;

        for kp in key_packages {
            let leaf_node = kp.leaf_node();
            let capabilities = leaf_node.capabilities();

            if !capabilities.extensions().contains(&extension_type) {
                return Err(GroupError::ProposalsNotSupported(
                    "Member does not support AppData dictionary: installation cannot receive standalone proposal messages".to_string(),
                ));
            }
        }

        Ok(())
    }

    // Create a new group and save it to the DB
    pub(crate) fn create_and_insert(
        context: Context,
        conversation_type: ConversationType,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
        oneshot_message: Option<OneshotMessage>,
    ) -> Result<Self, GroupError> {
        assert!(conversation_type != ConversationType::Dm);
        let stored_group = Self::insert(
            &context,
            None,
            GroupMembershipState::Allowed,
            conversation_type,
            permissions_policy_set,
            opts,
            oneshot_message,
            true,
        )?;
        let new_group = Self::new_from_arc(
            context.clone(),
            stored_group.id,
            stored_group.dm_id,
            conversation_type,
            stored_group.created_at_ns,
        );

        Ok(new_group)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "creation event mode keeps the write atomic"
    )]
    pub(crate) fn insert(
        context: &Context,
        existing_group_id: Option<&[u8]>,
        membership_state: GroupMembershipState,
        conversation_type: ConversationType,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
        oneshot_message: Option<OneshotMessage>,
        emit_created_event: bool,
    ) -> Result<StoredGroup, GroupError> {
        assert!(conversation_type != ConversationType::Dm);

        let creator_inbox_id = context.inbox_id();
        let commit_log_enabled = context.server_configuration().commit_log_enabled();
        // Deployments without a commit log do not create a signer.
        let signer =
            commit_log_enabled.then(xmtp_cryptography::rand::rand_secret::<ED25519_KEY_LENGTH>);
        let dictionary = initial_dictionary(
            InitialGroupKind::Group {
                conversation_type,
                oneshot_message: oneshot_message.as_ref(),
            },
            &permissions_policy_set
                .to_proto()
                .map_err(group_permissions::GroupMutablePermissionsError::from)
                .map_err(MetadataPermissionsError::from)?,
            &opts,
            creator_inbox_id,
            signer.as_ref().map(|key| key.as_slice()),
        )
        .map_err(app_data::migration::BootstrapSynthesisError::from)?;
        let group_config = build_group_config(dictionary)?;

        if !emit_created_event || conversation_type.is_virtual() {
            if emit_created_event && conversation_type == ConversationType::Sync {
                return crate::state_tx::state_write_with_events(
                    context.mls_storage(),
                    context.events(),
                    |tx, events| {
                        let (stored_group, created) = Self::insert_group_row(
                            context,
                            tx,
                            existing_group_id,
                            membership_state,
                            conversation_type,
                            &opts,
                            &group_config,
                            commit_log_enabled,
                        )?;
                        if created {
                            context.task_channels().mark_notification_changed();
                            events.emit(
                                None,
                                Some(crate::subscriptions::internal::InternalEvent::GroupJoined {
                                    group_id: stored_group.id,
                                    is_sync: true,
                                    origin: crate::subscriptions::internal::GroupOrigin::Created,
                                }),
                            );
                        }
                        Ok::<_, GroupError>(Continue(stored_group))
                    },
                )
                .map(TransactionOutcome::into_continued);
            }
            return state_write(context.mls_storage(), |tx| {
                let (stored_group, _) = Self::insert_group_row(
                    context,
                    tx,
                    existing_group_id,
                    membership_state,
                    conversation_type,
                    &opts,
                    &group_config,
                    commit_log_enabled,
                )?;
                Ok::<_, GroupError>(Continue(stored_group))
            })
            .map(TransactionOutcome::into_continued);
        }

        let result = crate::state_tx::state_write_with_events(
            context.mls_storage(),
            context.events(),
            |tx, events| {
                let (stored_group, created) = Self::insert_group_row(
                    context,
                    tx,
                    existing_group_id,
                    membership_state,
                    conversation_type,
                    &opts,
                    &group_config,
                    commit_log_enabled,
                )?;
                if !created {
                    return Ok::<_, GroupError>(Continue(stored_group));
                }
                let storage = tx.storage();
                let db = storage.db();
                let record = StoredConsentRecord::new(
                    xmtp_db::consent_record::ConsentType::ConversationId,
                    ConsentState::Allowed,
                    hex::encode(stored_group.id),
                );
                let consent_changes = db.insert_or_replace_consent_records(&[record])?;
                crate::subscriptions::internal::emit_preference_updates(
                    events,
                    consent_changes
                        .iter()
                        .cloned()
                        .map(PreferenceUpdate::Consent)
                        .collect(),
                    crate::subscriptions::internal::PreferenceOrigin::Local,
                    &db,
                )?;
                context.task_channels().mark_notification_changed();
                events.emit(
                    Some(xmtp_events::ClientEvent::ConversationJoined(
                        xmtp_events::ConversationJoined {
                            group_id: stored_group.id.to_vec(),
                            conversation_type: xmtp_events::ConversationType::Group,
                            origin: xmtp_events::JoinOrigin::Created,
                            adder_inbox_id: None,
                        },
                    )),
                    Some(crate::subscriptions::internal::InternalEvent::GroupJoined {
                        group_id: stored_group.id,
                        is_sync: false,
                        origin: crate::subscriptions::internal::GroupOrigin::Created,
                    }),
                );
                Ok::<_, GroupError>(Continue(stored_group))
            },
        )?
        .into_continued();
        Ok(result)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the row stores the creation parameters"
    )]
    fn insert_group_row(
        context: &Context,
        tx: &mut crate::state_tx::StateTx<'_, impl TransactionalKeyStore>,
        existing_group_id: Option<&[u8]>,
        membership_state: GroupMembershipState,
        conversation_type: ConversationType,
        opts: &GroupMetadataOptions,
        group_config: &MlsGroupCreateConfig,
        commit_log_enabled: bool,
    ) -> Result<(StoredGroup, bool), GroupError> {
        let storage = tx.storage();
        let db = storage.db();
        if let Some(existing_group_id) = existing_group_id {
            let group_id = GroupId::try_from(existing_group_id)?;
            if let Some(existing) = db.find_group(&group_id)? {
                return Ok((existing, false));
            }
        }
        let provider = XmtpOpenMlsProviderRef::new(&storage);
        let mls_group = if let Some(existing_group_id) = existing_group_id {
            // A restored group starts with a stub. A Welcome replaces it.
            OpenMlsGroup::from_backup_stub_logged(
                &provider,
                context.identity(),
                group_config,
                GroupId::try_from(existing_group_id)?,
                commit_log_enabled,
            )?
        } else {
            OpenMlsGroup::from_creation_logged(
                &provider,
                context.identity(),
                group_config,
                commit_log_enabled,
            )?
        };
        let group_id: GroupId = mls_group.group_id().try_into()?;
        let stored_group = StoredGroup::builder()
            .id(group_id)
            .created_at_ns(now_ns())
            .membership_state(membership_state)
            .conversation_type(conversation_type)
            .added_by_inbox_id(context.inbox_id().to_string())
            .message_disappear_from_ns(
                opts.message_disappearing_settings
                    .as_ref()
                    .map(|m| m.from_ns),
            )
            .message_disappear_in_ns(opts.message_disappearing_settings.as_ref().map(|m| m.in_ns))
            .should_publish_commit_log(existing_group_id.is_none())
            .build()?;
        stored_group.store_or_ignore(&db)?;
        Ok((stored_group, true))
    }

    // Create a new DM and save it to the DB
    pub(crate) fn create_dm_and_insert(
        context: &Context,
        membership_state: GroupMembershipState,
        dm_target_inbox_id: InboxId,
        opts: GroupMetadataOptions,
        existing_group_id: Option<&[u8]>,
    ) -> Result<Self, GroupError> {
        let commit_log_enabled = context.server_configuration().commit_log_enabled();
        let signer =
            commit_log_enabled.then(xmtp_cryptography::rand::rand_secret::<ED25519_KEY_LENGTH>);
        let dictionary = initial_dictionary(
            InitialGroupKind::Dm {
                target_inbox_id: &dm_target_inbox_id,
            },
            &PolicySet::new_dm()
                .to_proto()
                .map_err(group_permissions::GroupMutablePermissionsError::from)
                .map_err(MetadataPermissionsError::from)?,
            &opts,
            context.inbox_id(),
            signer.as_ref().map(|key| key.as_slice()),
        )
        .map_err(app_data::migration::BootstrapSynthesisError::from)?;
        let group_config = build_group_config(dictionary)?;

        let stored_group = if membership_state == GroupMembershipState::Restored {
            state_write(context.mls_storage(), |tx| {
                Ok::<_, GroupError>(Continue(Self::insert_dm_row(
                    context,
                    tx,
                    membership_state,
                    &dm_target_inbox_id,
                    &opts,
                    existing_group_id,
                    &group_config,
                    commit_log_enabled,
                )?))
            })?
            .into_continued()
            .0
        } else {
            crate::state_tx::state_write_with_events(
                context.mls_storage(),
                context.events(),
                |tx, events| {
                    let (stored_group, created, consent_changes) = Self::insert_dm_row(
                        context,
                        tx,
                        membership_state,
                        &dm_target_inbox_id,
                        &opts,
                        existing_group_id,
                        &group_config,
                        commit_log_enabled,
                    )?;
                    if created {
                        let storage = tx.storage();
                        let db = storage.db();
                        crate::subscriptions::internal::emit_preference_updates(
                            events,
                            consent_changes
                                .iter()
                                .cloned()
                                .map(PreferenceUpdate::Consent)
                                .collect(),
                            crate::subscriptions::internal::PreferenceOrigin::Local,
                            &db,
                        )?;
                        if existing_group_id.is_none() {
                            context.task_channels().mark_notification_changed();
                            events.emit(
                                Some(xmtp_events::ClientEvent::ConversationJoined(
                                    xmtp_events::ConversationJoined {
                                        group_id: stored_group.id.to_vec(),
                                        conversation_type: xmtp_events::ConversationType::Dm,
                                        origin: xmtp_events::JoinOrigin::Created,
                                        adder_inbox_id: None,
                                    },
                                )),
                                Some(crate::subscriptions::internal::InternalEvent::GroupJoined {
                                    group_id: stored_group.id,
                                    is_sync: false,
                                    origin: crate::subscriptions::internal::GroupOrigin::Created,
                                }),
                            );
                        }
                    }
                    Ok::<_, GroupError>(Continue((stored_group, created, consent_changes)))
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
    fn insert_dm_row(
        context: &Context,
        tx: &mut crate::state_tx::StateTx<'_, impl TransactionalKeyStore>,
        membership_state: GroupMembershipState,
        dm_target_inbox_id: &InboxId,
        opts: &GroupMetadataOptions,
        existing_group_id: Option<&[u8]>,
        group_config: &MlsGroupCreateConfig,
        commit_log_enabled: bool,
    ) -> Result<(StoredGroup, bool, Vec<StoredConsentRecord>), GroupError> {
        let storage = tx.storage();
        let db = storage.db();
        if let Some(group_id) = existing_group_id {
            let group_id = GroupId::try_from(group_id)?;
            if let Some(existing) = db.find_group(&group_id)? {
                return Ok((existing, false, Vec::new()));
            }
        }
        let provider = XmtpOpenMlsProviderRef::new(&storage);
        let mls_group = if let Some(group_id) = existing_group_id {
            OpenMlsGroup::from_backup_stub_logged(
                &provider,
                context.identity(),
                group_config,
                GroupId::try_from(group_id)?,
                commit_log_enabled,
            )?
        } else {
            OpenMlsGroup::from_creation_logged(
                &provider,
                context.identity(),
                group_config,
                commit_log_enabled,
            )?
        };
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
            .dm_id(Some(
                DmMembers {
                    member_one_inbox_id: dm_target_inbox_id.clone(),
                    member_two_inbox_id: context.identity().inbox_id().to_string(),
                }
                .to_string(),
            ))
            .build()?;
        stored_group.store(&db)?;
        let record = StoredConsentRecord::new(
            xmtp_db::consent_record::ConsentType::ConversationId,
            ConsentState::Allowed,
            hex::encode(group_id),
        );
        let consent_changes = db.insert_or_replace_consent_records(&[record])?;
        Ok((stored_group, true, consent_changes))
    }

    // Super admin status is only criteria for whether to publish the commit log for now
    pub(in crate::groups) fn check_should_publish_commit_log(
        inbox_id: String,
        mutable_metadata: Option<GroupMutableMetadata>,
    ) -> bool {
        mutable_metadata
            .as_ref()
            .map(|metadata| metadata.is_super_admin(&inbox_id))
            .unwrap_or(false) // Default to false if no mutable metadata
    }
}
