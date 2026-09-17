//! Mutable metadata, permissions, and group settings.

use super::*;

impl<Context> MlsGroup<Context>
where
    Context: XmtpSharedContext,
{
    /// Updates the name of the group. Will error if the user does not have the appropriate permissions
    /// to perform these updates.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn update_group_name(&self, group_name: String) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if group_name.len() > MAX_GROUP_NAME_LENGTH {
            return Err(GroupError::TooManyCharacters {
                length: MAX_GROUP_NAME_LENGTH,
            });
        }
        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_name(group_name).into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Set the group's opaque `app_data` slot.
    ///
    /// `expected_app_data` is an optional compare-and-swap guard. When
    /// `Some`, the update is abandoned with [`GroupError::AppDataSuperseded`]
    /// unless the committed value still equals it — including when another
    /// member's commit wins the epoch race *after* this intent was published.
    /// Callers reconciling structured state should pass the value they merged
    /// against, so a concurrent write is reported rather than overwritten.
    ///
    /// `None` keeps the historical last-writer-wins behavior: whatever landed
    /// in the meantime is overwritten.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn update_app_data(
        &self,
        app_data: String,
        expected_app_data: Option<String>,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if app_data.len() > MAX_APP_DATA_LENGTH {
            return Err(GroupError::TooManyCharacters {
                length: MAX_APP_DATA_LENGTH,
            });
        }
        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }

        // Fail the already-stale case before touching the network. The
        // authoritative check runs again at publish time, which is what
        // catches a change that lands between here and the commit.
        if let Some(expected) = &expected_app_data {
            // Read the slot itself rather than going through `app_data()`: a
            // group whose `app_data` has never been set is a legitimate "not
            // what you expected", and reporting it as `MissingExtension` would
            // hand the caller an error where it asked a question. A genuine
            // read failure still propagates — an unreadable group is not
            // evidence that someone else wrote the field.
            let actual = self.read_single_component::<AppDataComponent>()?;
            if actual.as_deref() != Some(expected.as_str()) {
                return Err(GroupError::AppDataSuperseded {
                    expected: expected.clone(),
                    // An unset slot reports as empty. The guard cannot yet
                    // *express* "I expect this to be unset" — the intent's
                    // `expected_field_value` is an optional string, where
                    // absent already means "no guard" — so an unset slot can
                    // only ever be a mismatch here, never an expectation.
                    actual: actual.unwrap_or_default(),
                });
            }
        }

        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_app_data(app_data, expected_app_data.clone())
                .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        match self.sync_until_intent_resolved(intent.id).await {
            Ok(_) => Ok(()),
            Err(err) => {
                // A guarded intent that lost the race is marked `Superseded`
                // rather than `Error`; translate it into the typed error so a
                // stale write is distinguishable from a genuine sync failure.
                if let Some(expected) = expected_app_data
                    && matches!(
                        self.context.db().fetch(&intent.id),
                        Ok(Some(StoredGroupIntent {
                            state: IntentState::Superseded,
                            ..
                        }))
                    )
                {
                    // Superseded is only set after the publish path read the
                    // committed value successfully, so this read should too;
                    // if it somehow fails, that error is the honest one.
                    return Err(GroupError::AppDataSuperseded {
                        expected,
                        actual: self.app_data()?,
                    });
                }
                Err(err)
            }
        }
    }

    /// Updates min version of the group to match this client's version.
    /// Not publicly exposed because:
    /// - Setting the min version to pre-release versions may not behave as expected
    /// - When the version is not explicitly specified, unexpected behavior may arise,
    ///   for example if the code is left in across multiple version bumps.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    #[allow(dead_code)]
    pub(crate) async fn update_group_min_version_to_match_self(&self) -> Result<(), GroupError> {
        let version = self.context.version_info().pkg_version();
        self.update_group_min_version(version).await
    }

    /// Updates min version of the group to match the given version.
    ///
    /// # Arguments
    /// * `version` - The libxmtp version to update the group min version to.
    ///   This is a semver-formatted string matching the Cargo.toml in the
    ///   libxmtp dependency, and does not match mobile or web release versions.
    ///   Comparison is done via the [`semver`] crate's `Ord` impl, so
    ///   pre-release identifiers (e.g. `"1.0.0-rc.1"`) sort BEFORE the
    ///   corresponding release (`"1.0.0"`) per semver 2.0 §11. Build
    ///   metadata (`+...`) parses but is included in ordering by the
    ///   semver crate — avoid passing it unless you understand the
    ///   total-ordering implication.
    ///
    /// # Returns
    /// A `Result` indicating success or failure of the operation.
    pub async fn update_group_min_version(&self, version: &str) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        // Footgun guards (apply on send side; receive side enforces
        // the same monotonicity invariant as the source of truth):
        //
        // 1. `version > own pkg_version` would pause this client (and
        //    every peer at or below this version) the moment the bump
        //    lands. Refuse.
        // 2. `version < current floor` would silently unpause peers
        //    between the new and old floors, defeating the gate. Refuse.
        //    Lenient on an unparseable current floor — mirror the
        //    receive-side behavior in `enforce_min_version_monotonicity`
        //    rather than have the send-side refuse where the receive-
        //    side accepts. (Brick recovery: a group with malformed
        //    legacy GMM bytes shouldn't be permanently un-bumpable.)
        let target_v =
            LibXMTPVersion::parse(version).map_err(|e| GroupError::InvalidMinVersion {
                value: version.to_string(),
                reason: e.to_string(),
            })?;
        let own_version_str = self.context.version_info().pkg_version().to_string();
        let own_v =
            LibXMTPVersion::parse(&own_version_str).map_err(|e| GroupError::InvalidMinVersion {
                value: own_version_str.clone(),
                reason: format!("own pkg_version: {e}"),
            })?;
        if target_v > own_v {
            return Err(GroupError::MinVersionExceedsOwnVersion {
                requested: version.to_string(),
                own: own_version_str,
            });
        }
        let current_str = self
            .mutable_metadata()?
            .attributes
            .get(MetadataField::MinimumSupportedProtocolVersion.as_str())
            .cloned();
        if let Some(current_str) = current_str.as_deref()
            && !current_str.is_empty()
        {
            match LibXMTPVersion::parse(current_str) {
                Ok(current_v) => {
                    if target_v < current_v {
                        return Err(GroupError::MinVersionDowngrade {
                            requested: version.to_string(),
                            current: current_str.to_string(),
                        });
                    }
                }
                Err(e) => {
                    // Observability: malformed prior floor is an operator-
                    // visible signal that the legacy GMM bytes are corrupt.
                    // Leniency below preserves brick-recovery; the warning
                    // surfaces the corruption.
                    tracing::warn!(
                        current = %current_str,
                        error = %e,
                        "update_group_min_version: existing min_version is unparseable; \
                         proceeding without downgrade check"
                    );
                }
            }
        }

        tracing::info!("update_group_min_version: queuing bump to {}", version);
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_min_version_to_match_self(
                version.to_string(),
            )
            .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Updates the commit log signer of the group. Will error if the user does not have the appropriate permissions
    /// to perform these updates.
    pub async fn update_commit_log_signer(
        &self,
        commit_log_signer: xmtp_cryptography::Secret,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_commit_log_signer(commit_log_signer).into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    pub(in crate::groups) fn min_protocol_version_from_extensions(
        mutable_metadata: &GroupMutableMetadata,
    ) -> Option<String> {
        mutable_metadata
            .attributes
            .get(&MetadataField::MinimumSupportedProtocolVersion.to_string())
            .map(|v| v.to_string())
    }

    /// Updates the permission policy of the group. This requires super admin permissions.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn update_permission_policy(
        &self,
        permission_update_type: PermissionUpdateType,
        permission_policy: PermissionPolicyOption,
        metadata_field: Option<MetadataField>,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        if permission_update_type == PermissionUpdateType::UpdateMetadata
            && metadata_field.is_none()
        {
            return Err(MetadataPermissionsError::InvalidPermissionUpdate.into());
        }

        if matches!(
            permission_update_type,
            PermissionUpdateType::AddAdmin | PermissionUpdateType::RemoveAdmin
        ) && permission_policy == PermissionPolicyOption::Allow
        {
            return Err(MetadataPermissionsError::InvalidPermissionUpdate.into());
        }

        let intent_data: Vec<u8> = UpdatePermissionIntentData::new(
            permission_update_type,
            permission_policy,
            metadata_field.as_ref().map(|field| field.to_string()),
        )
        .into();

        let intent = QueueIntent::update_permission()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Retrieves the group name from the group's mutable metadata extension.
    pub fn group_name(&self) -> Result<String, GroupError> {
        Ok(self
            .read_single_component::<GroupNameComponent>()?
            .unwrap_or_default())
    }

    /// Retrieves the app_data field from the group's mutable metadata extension
    pub fn app_data(&self) -> Result<String, GroupError> {
        Ok(self
            .read_single_component::<AppDataComponent>()?
            .unwrap_or_default())
    }

    /// Updates the description of the group.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn update_group_description(
        &self,
        group_description: String,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if group_description.len() > MAX_GROUP_DESCRIPTION_LENGTH {
            return Err(GroupError::TooManyCharacters {
                length: MAX_GROUP_DESCRIPTION_LENGTH,
            });
        }

        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_description(group_description).into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    pub fn group_description(&self) -> Result<String, GroupError> {
        Ok(self
            .read_single_component::<GroupDescriptionComponent>()?
            .unwrap_or_default())
    }

    /// Updates the image URL (square) of the group.
    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub async fn update_group_image_url_square(
        &self,
        group_image_url_square: String,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        if group_image_url_square.len() > MAX_GROUP_IMAGE_URL_LENGTH {
            return Err(GroupError::TooManyCharacters {
                length: MAX_GROUP_IMAGE_URL_LENGTH,
            });
        }

        if self.metadata().await?.conversation_type == ConversationType::Dm {
            return Err(MetadataPermissionsError::DmGroupMetadataForbidden.into());
        }
        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_group_image_url_square(group_image_url_square)
                .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;

        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// Retrieves the image URL (square) of the group from the group's mutable metadata extension.
    pub fn group_image_url_square(&self) -> Result<String, GroupError> {
        Ok(self
            .read_single_component::<GroupImageUrlComponent>()?
            .unwrap_or_default())
    }

    pub async fn update_conversation_message_disappearing_settings(
        &self,
        settings: MessageDisappearingSettings,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        self.update_conversation_message_disappear_from_ns(settings.from_ns)
            .await?;
        self.update_conversation_message_disappear_in_ns(settings.in_ns)
            .await
    }

    pub async fn remove_conversation_message_disappearing_settings(
        &self,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        self.update_conversation_message_disappearing_settings(
            MessageDisappearingSettings::default(),
        )
        .await
    }

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub(in crate::groups) async fn update_conversation_message_disappear_from_ns(
        &self,
        expire_from_ms: i64,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_conversation_message_disappear_from_ns(
                expire_from_ms,
            )
            .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;
        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    #[cfg_attr(any(test, feature = "test-utils"), tracing::instrument(level = "info", fields(inbox_id = %self.context.inbox_id()), skip(self)))]
    #[cfg_attr(
        not(any(test, feature = "test-utils")),
        tracing::instrument(level = "trace", skip(self))
    )]
    pub(in crate::groups) async fn update_conversation_message_disappear_in_ns(
        &self,
        expire_in_ms: i64,
    ) -> Result<(), GroupError> {
        self.ensure_not_paused().await?;

        let intent_data: Vec<u8> =
            UpdateMetadataIntentData::new_update_conversation_message_disappear_in_ns(expire_in_ms)
                .into();
        let intent = QueueIntent::metadata_update()
            .data(intent_data)
            .queue(self)?;
        let _ = self.sync_until_intent_resolved(intent.id).await?;
        Ok(())
    }

    /// If group is not paused, will return None, otherwise will return the version that the group is paused for
    pub fn paused_for_version(&self) -> Result<Option<String>, GroupError> {
        let paused_for_version = self.context.db().get_group_paused_version(&self.group_id)?;
        Ok(paused_for_version)
    }

    #[tracing::instrument(skip_all, level = "trace")]
    pub(in crate::groups) async fn ensure_not_paused(&self) -> Result<(), GroupError> {
        if let Some(min_version) = self.context.db().get_group_paused_version(&self.group_id)? {
            Err(GroupError::GroupPausedUntilUpdate(min_version))
        } else {
            Ok(())
        }
    }

    pub fn conversation_message_disappearing_settings(
        &self,
    ) -> Result<MessageDisappearingSettings, GroupError> {
        let metadata = self.mutable_metadata()?;
        Self::conversation_message_disappearing_settings_from_extensions(&metadata)
    }

    pub fn conversation_message_disappearing_settings_from_extensions(
        mutable_metadata: &GroupMutableMetadata,
    ) -> Result<MessageDisappearingSettings, GroupError> {
        let disappear_from_ns = mutable_metadata
            .attributes
            .get(&MetadataField::MessageDisappearFromNS.to_string());
        let disappear_in_ns = mutable_metadata
            .attributes
            .get(&MetadataField::MessageDisappearInNS.to_string());

        if let (Some(Ok(message_disappear_from_ns)), Some(Ok(message_disappear_in_ns))) = (
            disappear_from_ns.map(|s| s.parse::<i64>()),
            disappear_in_ns.map(|s| s.parse::<i64>()),
        ) {
            Ok(MessageDisappearingSettings::new(
                message_disappear_from_ns,
                message_disappear_in_ns,
            ))
        } else {
            Err(GroupError::MetadataPermissionsError(
                GroupMetadataError::MissingExtension.into(),
            ))
        }
    }

    pub fn pending_remove_list(&self) -> Result<Vec<String>, GroupError> {
        self.context
            .db()
            .get_pending_remove_users(&self.group_id)
            .map_err(Into::into)
    }

    /// Checks if the given inbox ID is the pending-remove list of the group at the most recently synced epoch.
    pub fn is_in_pending_remove(&self, inbox_id: &str) -> Result<bool, GroupError> {
        self.context
            .db()
            .get_user_pending_remove_status(&self.group_id, inbox_id)
            .map_err(Into::into)
    }

    /// Retrieves the admin list of the group from the group's mutable metadata extension.
    ///
    /// Element order: on migrated groups the dict-backed `TlsSet<InboxId>`
    /// is iterated in sorted-by-raw-bytes order. On unmigrated groups
    /// the legacy `GroupMutableMetadata.admin_list` is returned in its
    /// stored (insertion) order. Both contracts pre-date this refactor;
    /// preserving each side avoids surprising binding consumers that
    /// rely on the pre-migration order.
    pub fn admin_list(&self) -> Result<Vec<String>, GroupError> {
        self.read_admin_set_preserving_legacy_order(AdminListKind::Admin)
    }

    /// Retrieves the super admin list of the group from the group's mutable metadata extension.
    ///
    /// Same ordering contract as [`Self::admin_list`].
    pub fn super_admin_list(&self) -> Result<Vec<String>, GroupError> {
        self.read_admin_set_preserving_legacy_order(AdminListKind::SuperAdmin)
    }

    fn read_admin_set_preserving_legacy_order(
        &self,
        kind: AdminListKind,
    ) -> Result<Vec<String>, GroupError> {
        let ctx = self.load_group_context()?;
        let extensions = ctx.extensions();
        if self::app_data::is_migrated_extensions(extensions) {
            let facade = self::app_data::typed_facade::MlsGroupAppData::new(extensions);
            let set = match kind {
                AdminListKind::Admin => facade.get::<AdminListComponent>(),
                AdminListKind::SuperAdmin => facade.get::<SuperAdminListComponent>(),
            }
            .map_err(|e| {
                GroupError::MetadataPermissionsError(MetadataPermissionsError::ComponentSource(e))
            })?;
            Ok(set
                .map(|s| s.iter().map(|id| id.to_hex()).collect())
                .unwrap_or_default())
        } else {
            // Unmigrated: return the Vec<String> straight from the legacy GMM
            // extension so callers keep their pre-migration insertion order.
            // Propagate decode errors (e.g. a corrupted legacy GMM extension)
            // via the same `MetadataPermissionsError::Mutable(...)` shape that
            // pre-refactor `mutable_metadata()?.admin_list` produced — a soft
            // `.ok()` here would convert a loud failure into silent "no admins"
            // data corruption. `MissingExtension` is the legacy "no extension on
            // the group" case and remains the soft-skip → empty Vec contract.
            let metadata = match xmtp_mls_common::group_mutable_metadata::extract_legacy_group_mutable_metadata_from_extensions(
                extensions,
            ) {
                Ok(m) => Some(m),
                Err(xmtp_mls_common::group_mutable_metadata::GroupMutableMetadataError::MissingExtension) => {
                    // Expected on very old groups created before the legacy GMM
                    // extension existed; logged at debug to give operators
                    // visibility without spamming warn on a legitimate state. An
                    // empty list is the contract callers expect (admin_list /
                    // super_admin_list return `Ok(vec![])` here, not `Err`).
                    tracing::debug!(
                        group_id = %self.group_id,
                        kind = ?kind,
                        "unmigrated group has no legacy GroupMutableMetadata extension; returning empty admin set"
                    );
                    None
                }
                Err(e) => {
                    return Err(GroupError::MetadataPermissionsError(
                        MetadataPermissionsError::Mutable(e),
                    ));
                }
            };
            Ok(metadata
                .map(|m| match kind {
                    AdminListKind::Admin => m.admin_list,
                    AdminListKind::SuperAdmin => m.super_admin_list,
                })
                .unwrap_or_default())
        }
    }

    /// Checks if the given inbox ID is an admin of the group at the most recently synced epoch.
    pub fn is_admin(&self, inbox_id: String) -> Result<bool, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.admin_list.contains(&inbox_id))
    }

    /// Checks if the given inbox ID is a super admin of the group at the most recently synced epoch.
    pub fn is_super_admin(&self, inbox_id: String) -> Result<bool, GroupError> {
        let mutable_metadata = self.mutable_metadata()?;
        Ok(mutable_metadata.super_admin_list.contains(&inbox_id))
    }

    /// Checks if the given inbox ID is a super admin of the group at the most recently synced epoch
    pub fn is_super_admin_without_lock(
        &self,
        mls_group: &OpenMlsGroup,
        inbox_id: String,
    ) -> Result<bool, GroupMutableMetadataError> {
        // On migrated groups, the legacy GMM extension is gone — read
        // SUPER_ADMIN_LIST from the AppData dict. A missing dict
        // entry on a migrated group is treated as "no super-admins":
        // falling through to `GroupMutableMetadata::try_from(mls_group)`
        // would hit `MissingExtension` because bootstrap has already
        // stripped the legacy GMM. Today bootstrap always seeds an
        // (empty or populated) `SUPER_ADMIN_LIST` entry so the `None`
        // branch is defensive, but the explicit handling keeps the
        // read-side safe against any future weakening of that
        // invariant.
        //
        // On unmigrated groups we fall back to the legacy GMM
        // extension — that path is unchanged.
        if self::app_data::is_migrated_group(mls_group) {
            let list = self::app_data::component_source::read_super_admin_list_from_dict(mls_group)
                .map_err(GroupMutableMetadataError::from)?
                .unwrap_or_default();
            return Ok(list.contains(&inbox_id));
        }
        let mutable_metadata = GroupMutableMetadata::try_from(mls_group)?;
        Ok(mutable_metadata.super_admin_list.contains(&inbox_id))
    }

    /// Retrieves the conversation type of the group from the group's metadata extension.
    pub async fn conversation_type(&self) -> Result<ConversationType, GroupError> {
        let conversation_type = self.context.db().get_conversation_type(&self.group_id)?;
        Ok(conversation_type)
    }
}
