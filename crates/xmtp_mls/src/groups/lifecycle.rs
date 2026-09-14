//! Construction, loading, proposal capability, and insertion.

use super::*;

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

    /// Check published capabilities using immutable member IDs from one snapshot.
    async fn ensure_members_support_proposals(&self) -> Result<(), GroupError> {
        let (supported, installation_ids) =
            self.with_group_snapshot(|group| Ok(self.proposal_support_snapshot(group)))?;
        if self
            .published_members_support_proposals(supported, installation_ids)
            .await?
        {
            Ok(())
        } else {
            Err(GroupError::ProposalsNotSupported(
                "Cannot enable proposals: not all members support the proposal extension"
                    .to_string(),
            ))
        }
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

    /// Check if the group has proposals enabled (proposal-by-reference flow).
    ///
    /// Delegates to `check_proposals_enabled` which detects the
    /// standard MLS `ExtensionType::AppDataDictionary` group-context
    /// extension. A migrated group carries the dict via that extension
    /// type, making it both the wire-format carrier AND the signal
    /// that the proposal flow is in effect.
    ///
    /// When proposals are enabled on a group:
    /// - Add/remove member operations MUST use proposals
    /// - All members being added MUST advertise `AppDataDictionary`
    ///   support in their key-package capabilities
    /// - Direct commits for membership changes are not allowed
    pub fn proposals_enabled(&self, mls_group: &OpenMlsGroup) -> bool {
        check_proposals_enabled(mls_group.extensions())
    }

    /// Like [`Self::proposals_enabled`], but loads the group from storage
    /// instead of taking a caller-held `OpenMlsGroup`. The convenience
    /// shape bindings need for a plain "is this group migrated?" read.
    pub fn is_proposals_enabled(&self) -> Result<bool, GroupError> {
        self.with_group_snapshot(|mls_group| Ok(self.proposals_enabled(mls_group)))
    }

    /// Enable proposals on this group (proposal-by-reference flow).
    ///
    /// Runs the bootstrap commit that migrates the group's state out
    /// of legacy GMM-style extensions and into the standard MLS
    /// `AppDataDictionary` extension. After bootstrap completes the
    /// dictionary is the sole source of truth for the metadata
    /// attributes that previously lived in the legacy extensions.
    /// Once enabled:
    /// - All add/remove member operations will use proposals
    /// - All members being added must advertise `AppDataDictionary`
    ///   support in their key-package capabilities
    /// - This cannot be disabled once set
    ///
    /// # Options
    ///
    /// See [`EnableProposalsOptions`] for the two knobs:
    /// - `force`: skip the pre-flight key-package capability check. Use
    ///   when the version floor guarantees proposal support.
    /// - `min_version`: override the `MIN_SUPPORTED_PROTOCOL_VERSION`
    ///   floor written into the migrated group. Defaults to
    ///   [`xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION`] — the
    ///   release where proposals support first ships.
    ///
    /// # Prerequisites
    ///
    /// Before calling this method with `force = false`, ensure all
    /// existing members support proposals by calling
    /// `all_members_support_proposals()`.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `force = false` and not all existing members support proposals
    /// - `min_version` is set to an invalid semver string
    /// - `min_version` is outside the allowed bounds: above the
    ///   caller's own `pkg_version`, or — in non-test builds — below
    ///   [`xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION`]
    /// - The group context extension update fails
    pub async fn enable_proposals(
        &self,
        options: EnableProposalsOptions,
    ) -> Result<(), GroupError> {
        // Race-loss recovery: a concurrent migrator may win the
        // race, advancing the group's epoch and causing our own
        // intent's commit to fail when it tries to apply locally.
        // The user-facing semantic of `enable_proposals` is "after
        // this returns Ok, the group is migrated" — so if we error
        // out but the group ended up migrated anyway, that's a
        // benign race loss, not a failure.
        //
        // Implementation: delegate to an inner function and, on
        // error, check `proposals_enabled` one final time. Only
        // genuine failures (where the group is NOT migrated) are
        // surfaced to the caller.
        let result = self.enable_proposals_inner(options).await;
        if let Err(ref err) = result {
            let migrated_anyway = self
                .with_group_snapshot(|mls_group| {
                    Ok::<bool, GroupError>(self.proposals_enabled(mls_group))
                })
                .unwrap_or_else(|check_err| {
                    tracing::warn!(
                        inbox_id = self.context.inbox_id(),
                        group_id = hex::encode(self.group_id.as_ref()),
                        error = %check_err,
                        "enable_proposals: recovery-path migration check failed; \
                         falling back to surfacing original error"
                    );
                    false
                });
            if migrated_anyway {
                tracing::info!(
                    inbox_id = self.context.inbox_id(),
                    group_id = hex::encode(self.group_id.as_ref()),
                    error = %err,
                    "enable_proposals: error surfaced but group is migrated — \
                     treating as success (concurrent migrator won the race)"
                );
                return Ok(());
            }
        }
        result
    }

    async fn enable_proposals_inner(
        &self,
        options: EnableProposalsOptions,
    ) -> Result<(), GroupError> {
        // Two-step migration so old clients (any peer running a
        // libxmtp release predating this code's `pkg_version`) get a
        // pause hint they can read BEFORE the legacy
        // GroupMutableMetadata extension that carries it is stripped
        // by the bootstrap commit. XIP §3.2.
        //
        // Step A: bump MIN_SUPPORTED_PROTOCOL_VERSION in legacy GMM to
        // `pkg_version()`. Pre-bootstrap groups have no AppData
        // dictionary registry, so the `MetadataUpdate` handler's
        // migrated branch is false and the write goes through the
        // legacy GCE path — exactly the extension old clients know how
        // to read for `paused_for_version`. Any peer below the version
        // floor processes this commit, sees the version mismatch in
        // `validate_one_commit`, and lands in `paused_for_version`. It
        // never observes step B.
        //
        // Step B: bootstrap commit. Strips legacy extensions, seeds
        // the dict, adds `AppDataDictionary` to RequiredCapabilities.
        // Fires only for the still-active (above-floor) members.
        //
        // The safety primitive here is **server-side commit ordering**:
        // peers see step A's commit before step B's because the server
        // linearizes them. `sync_until_intent_resolved` confirms only
        // the migrator's local Processed state — it does NOT wait for
        // peer pickup. Server linearization is what guarantees
        // below-floor peers pause before they could ever see the
        // bootstrap commit.
        //
        // Pre-flight: read all three gating signals under one lock so
        // a concurrent migrator's bootstrap commit can't land between
        // our reads. Without the single-lock pass:
        //   * member-support check passes, then a concurrent migrator's
        //     bootstrap commit lands locally, then the legacy-GMM read
        //     returns `None` (extension stripped), then `needs_bump`
        //     becomes `true`, then step A publishes a legacy GCE bump
        //     against a group whose legacy extensions are gone — fails
        //     mid-flight with a confusing error.
        // Reading all three together collapses that race window: if
        // `proposals_enabled` flipped to true under us, we early-return
        // and never publish step A.
        //
        // (1) `all_members_support_proposals` ensures every peer can
        // process the bootstrap commit so we don't ship step A — the
        // legacy GMM bump — for a migration that's about to fail at
        // step B and leave below-floor peers permanently paused. Gated
        // by `options.force`: when the version floor guarantees support, callers can
        // explicitly opt out of the per-member scan.
        // (2) `proposals_enabled` early-exits if the group is already
        // migrated; calling `enable_proposals()` twice is a user error
        // and we don't want to publish a redundant legacy GMM bump on
        // a group that no longer has a legacy GMM.
        // (3) `needs_min_version_bump` decides whether step A is
        // necessary. Semver comparison, NOT string equality. A
        // concurrent migrator at a higher version (or this very
        // migrator on retry) may have already set a floor >= ours; in
        // that case re-bumping with a lower string value would
        // downgrade the floor and silently unpause peers between the
        // lower and the higher version. Skip step A when the current
        // floor already covers the target.
        let min_version = options
            .min_version
            .unwrap_or_else(|| xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION.to_string());
        // Validate the floor string up-front so we fail with a clear
        // error before publishing the legacy GMM bump rather than
        // discovering it mid-flight when the validator parses it.
        let target_v =
            LibXMTPVersion::parse(&min_version).map_err(|e| GroupError::InvalidMinVersion {
                value: min_version.clone(),
                reason: e.to_string(),
            })?;
        let own_version_str = self.context.version_info().pkg_version().to_string();
        let own_v =
            LibXMTPVersion::parse(&own_version_str).map_err(|e| GroupError::InvalidMinVersion {
                value: own_version_str.clone(),
                reason: format!("own pkg_version: {e}"),
            })?;
        let force = options.force;

        log_event!(
            Event::EnableProposalsStart,
            self.context.installation_id(),
            group_id = self.group_id,
            min_version = min_version.as_str(),
            force
        );
        if !force {
            self.ensure_members_support_proposals().await?;
        }
        let (already_migrated, needs_min_version_bump) = self
            .with_group_snapshot(|mls_group| {
                // Idempotency: re-calling enable_proposals on an
                // already-migrated group is a no-op success.
                // The footgun clamp below MUST run after this
                // early-return — otherwise a caller pinning a
                // forward-looking constant in idempotent retry code
                // would error post-migration even when the floor was
                // already set by the original call.
                if self.proposals_enabled(mls_group) {
                    return Ok::<(bool, bool), GroupError>((true, false));
                }
                // Footgun guard: a caller setting min_version above
                // their own pkg_version would pause themselves (and
                // every peer at or below their version) the moment the
                // bump landed — bricking the group from the inside.
                // Refuse. Honest mistakes only; a malicious client can
                // patch this out, but honest mistakes are what we're
                // protecting against.
                if target_v > own_v {
                    return Err(GroupError::MinVersionExceedsOwnVersion {
                        requested: min_version.clone(),
                        own: own_version_str.clone(),
                    });
                }
                // Encoder-freeze clamp (lower bound): the bootstrap
                // encoder is byte-frozen, and receive-side validators
                // accept its output via strict byte-compare only down
                // to `PROPOSALS_MIN_PROTOCOL_VERSION`. Seeding a floor
                // below that constant would drop below-floor receivers
                // — whose frozen decoder may disagree on the bytes —
                // back into the byte-compare instead of the pause
                // path, reopening the fork the floor exists to close.
                // Refuse, so the effective invariant is
                // `PROPOSALS_MIN_PROTOCOL_VERSION <= min_version <=
                // own pkg_version`. Test builds are exempt so
                // `EnableProposalsOptions::test_default()` can seed a
                // synthetic below-floor value (`"0.0.0"`) that never
                // pauses workspace-version peers.
                #[cfg(not(any(test, feature = "test-utils")))]
                {
                    let floor_str = xmtp_configuration::PROPOSALS_MIN_PROTOCOL_VERSION;
                    let floor_v = LibXMTPVersion::parse(floor_str).map_err(|e| {
                        GroupError::InvalidMinVersion {
                            value: floor_str.to_string(),
                            reason: format!("PROPOSALS_MIN_PROTOCOL_VERSION: {e}"),
                        }
                    })?;
                    if target_v < floor_v {
                        return Err(GroupError::MinVersionDowngrade {
                            requested: min_version.clone(),
                            current: floor_str.to_string(),
                        });
                    }
                }
                let metadata =
                    xmtp_mls_common::group_mutable_metadata::extract_legacy_group_mutable_metadata(
                        mls_group,
                    )
                    .ok();
                let current = metadata.and_then(|m| Self::min_protocol_version_from_extensions(&m));
                let needs_bump = match current.as_deref() {
                    None => true,
                    Some(current_str) => match LibXMTPVersion::parse(current_str) {
                        Ok(current_v) => current_v < target_v,
                        Err(e) => {
                            // Lenient on a malformed legacy GMM floor (mirrors the
                            // receive-side leniency in
                            // [`enforce_min_version_monotonicity`]). Step A
                            // overwrites the value entirely with a known-good
                            // semver string, so a malformed prior can't poison the
                            // bump. Log a warning so operators can detect
                            // corrupted state.
                            tracing::warn!(
                                current = %current_str,
                                error = %e,
                                "enable_proposals: legacy GMM MinimumSupportedProtocolVersion is unparseable; \
                                 proceeding with step-A bump to overwrite"
                            );
                            true
                        }
                    },
                };
                Ok::<(bool, bool), GroupError>((false, needs_bump))
            })?;

        if already_migrated {
            log_event!(
                Event::EnableProposalsCompleted,
                self.context.installation_id(),
                group_id = self.group_id,
                already_migrated = true,
                min_version = min_version.as_str()
            );
            return Ok(());
        }

        if needs_min_version_bump {
            let min_version_intent_data: Vec<u8> =
                intents::UpdateMetadataIntentData::new_update_group_min_version_to_match_self(
                    min_version.clone(),
                )
                .into();
            let min_version_intent = intents::QueueIntent::metadata_update()
                .data(min_version_intent_data)
                .queue(self)?;
            self.sync_until_intent_resolved(min_version_intent.id)
                .await?;
        }

        // Build the bootstrap-target extensions in a single lock
        // acquisition to avoid races. The bootstrap commit produced by
        // `IntentKind::BootstrapMigration` will:
        // - REMOVE the four legacy XMTP extensions (mutable metadata,
        //   group permissions, group membership, ImmutableMetadata)
        // - update RequiredCapabilities to add `AppDataDictionary` and
        //   drop the four legacy extension types
        // - emit one `AppDataUpdate(component_id, Update(bytes))`
        //   proposal per well-known component, seeding the dict — the
        //   AppDataDictionary GCE itself is populated by openmls when
        //   the bundled AppDataUpdate proposals apply during commit
        //   processing
        // All in a single commit, so the migration is atomic on-the-
        // wire: receivers either see the migrated state (bootstrap
        // commit accepted) or the legacy state (commit rejected).
        //
        // The `all_members_support_proposals` re-check on this read
        // pass is a defense-in-depth: pre-flight ran above before step
        // A, but a peer could join between then and now. The intent
        // dispatch reads live group state at publish time, so step B
        // must observe the same predicate it gated step A with. Same
        // `force` gate as the pre-flight — if the caller explicitly
        // disabled the capability check there, honor that here too so
        // a freshly-joined member can't re-block the migration mid-
        // flight.
        if !force {
            self.ensure_members_support_proposals().await?;
        }
        let new_extensions = self.with_group_snapshot(|mls_group| {
            // Re-check `proposals_enabled` inside the lock: a
            // concurrent migrator may have completed the migration
            // between the first idempotency check and this second
            // lock acquisition. Returning `None` here lets the
            // outer code skip queuing a redundant bootstrap intent
            // — preserves the idempotency contract documented at
            // the top of `enable_proposals_inner`.
            if self.proposals_enabled(mls_group) {
                return Ok::<Option<Extensions<GroupContext>>, GroupError>(None);
            }
            let mut extensions: Extensions<GroupContext> = mls_group.extensions().clone();

            // 1. Remove the four legacy XMTP extensions. The
            //    bootstrap commit's job is to eliminate them so
            //    the dict becomes the sole source of truth.
            //    The bundled `AppDataUpdate(COMPONENT_REGISTRY)`
            //    proposal triggers openmls to add the standard
            //    `AppDataDictionary` group-context extension when
            //    the commit applies — that extension's presence
            //    (plus the registry entry) IS the migrated marker.
            //    No separate XMTP-flavored marker is needed.
            extensions.remove(ExtensionType::Unknown(MUTABLE_METADATA_EXTENSION_ID));
            extensions.remove(ExtensionType::Unknown(GROUP_PERMISSIONS_EXTENSION_ID));
            extensions.remove(ExtensionType::Unknown(GROUP_MEMBERSHIP_EXTENSION_ID));
            extensions.remove(ExtensionType::ImmutableMetadata);

            // 2. Update RequiredCapabilities: require
            //    `AppDataDictionary` (the standard extension that
            //    carries the dict) and drop the four legacy
            //    extension types so receivers don't reject the
            //    commit for missing required extensions.
            update_required_capabilities_for_bootstrap(&mut extensions)?;

            Ok(Some(extensions))
        })?;

        // Concurrent migrator won the race between the two lock
        // acquisitions — group is already migrated, no bootstrap
        // intent to queue.
        let Some(new_extensions) = new_extensions else {
            return Ok(());
        };

        use openmls::prelude::tls_codec::Serialize;
        let extensions_bytes = new_extensions.tls_serialize_detached()?;

        // Queue the bootstrap intent. The handler synthesizes
        // per-component dict seeds and bundles the GCE proposal +
        // every AppDataUpdate proposal into one self-contained
        // commit. No follow-up CommitPendingProposals needed.
        let intent_data = intents::ProposeGroupContextExtensionsIntentData::new(extensions_bytes);
        let bootstrap_intent = intents::QueueIntent::bootstrap_migration()
            .data(intent_data)
            .queue(self)?;

        self.sync_until_intent_resolved(bootstrap_intent.id).await?;

        let enabled = self.with_group_snapshot(|mls_group| {
            Ok::<bool, GroupError>(self.proposals_enabled(mls_group))
        })?;

        if !enabled {
            return Err(GroupError::ProposalsNotSupported(
                "Failed to enable proposals: extension not applied".to_string(),
            ));
        }

        log_event!(
            Event::EnableProposalsCompleted,
            self.context.installation_id(),
            group_id = self.group_id,
            already_migrated = false,
            min_version = min_version.as_str()
        );
        Ok(())
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
        )?;
        let new_group = Self::new_from_arc(
            context.clone(),
            stored_group.id,
            stored_group.dm_id,
            conversation_type,
            stored_group.created_at_ns,
        );

        // Consent state defaults to allowed when the user creates the group
        if !conversation_type.is_virtual() {
            new_group.update_consent_state(ConsentState::Allowed)?;
        }

        Ok(new_group)
    }

    pub(crate) fn insert(
        context: &Context,
        existing_group_id: Option<&[u8]>,
        membership_state: GroupMembershipState,
        conversation_type: ConversationType,
        permissions_policy_set: PolicySet,
        opts: GroupMetadataOptions,
        oneshot_message: Option<OneshotMessage>,
    ) -> Result<StoredGroup, GroupError> {
        assert!(conversation_type != ConversationType::Dm);

        let creator_inbox_id = context.inbox_id();
        let protected_metadata = build_protected_metadata_extension(
            creator_inbox_id,
            conversation_type,
            oneshot_message,
        )?;
        let mutable_metadata =
            build_mutable_metadata_extension_default(creator_inbox_id, opts.clone())?;
        let group_membership = build_starting_group_membership_extension(creator_inbox_id, 0);
        let mutable_permissions = build_mutable_permissions_extension(permissions_policy_set)?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permissions,
        )?;

        state_write(context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            if let Some(existing_group_id) = existing_group_id {
                let group_id = GroupId::try_from(existing_group_id)?;
                if let Some(existing) = db.find_group(&group_id)? {
                    return Ok(Continue(existing));
                }
            }
            let provider = XmtpOpenMlsProviderRef::new(&storage);
            let mls_group = if let Some(existing_group_id) = existing_group_id {
                // TODO: For groups restored from backup, in order to support queries on metadata such as
                // the group title and description, a stubbed OpenMLS group is created, and later overwritten
                // when a welcome is received.
                OpenMlsGroup::from_backup_stub_logged(
                    &provider,
                    context.identity(),
                    &group_config,
                    GroupId::try_from(existing_group_id)?,
                )?
            } else {
                OpenMlsGroup::from_creation_logged(&provider, context.identity(), &group_config)?
            };

            let group_id: GroupId = mls_group.group_id().try_into()?;
            // If not an existing group, the creator is a super admin and should publish the commit log
            // Otherwise, for existing groups, we'll never publish the commit log until we receive a welcome message
            let should_publish_commit_log = existing_group_id.is_none();

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
                .message_disappear_in_ns(
                    opts.message_disappearing_settings.as_ref().map(|m| m.in_ns),
                )
                .should_publish_commit_log(should_publish_commit_log)
                .build()?;

            stored_group.store_or_ignore(&db)?;
            Ok::<_, GroupError>(Continue(stored_group))
        })
        .map(TransactionOutcome::into_continued)
    }

    // Create a new DM and save it to the DB
    pub(crate) fn create_dm_and_insert(
        context: &Context,
        membership_state: GroupMembershipState,
        dm_target_inbox_id: InboxId,
        opts: DMMetadataOptions,
        existing_group_id: Option<&[u8]>,
    ) -> Result<Self, GroupError> {
        let protected_metadata =
            build_dm_protected_metadata_extension(context.inbox_id(), dm_target_inbox_id.clone())?;
        let mutable_metadata = build_dm_mutable_metadata_extension_default(
            context.inbox_id(),
            &dm_target_inbox_id,
            opts.clone(),
        )?;
        let group_membership = build_starting_group_membership_extension(context.inbox_id(), 0);
        let mutable_permissions = PolicySet::new_dm();
        let mutable_permission_extension =
            build_mutable_permissions_extension(mutable_permissions)?;
        let group_config = build_group_config(
            protected_metadata,
            mutable_metadata,
            group_membership,
            mutable_permission_extension,
        )?;

        let (stored_group, created) = state_write(context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            if let Some(group_id) = existing_group_id {
                let group_id = GroupId::try_from(group_id)?;
                if let Some(existing) = db.find_group(&group_id)? {
                    return Ok(Continue((existing, false)));
                }
            }
            let provider = XmtpOpenMlsProviderRef::new(&storage);
            let mls_group = if let Some(group_id) = existing_group_id {
                OpenMlsGroup::from_backup_stub_logged(
                    &provider,
                    context.identity(),
                    &group_config,
                    GroupId::try_from(group_id)?,
                )?
            } else {
                OpenMlsGroup::from_creation_logged(&provider, context.identity(), &group_config)?
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
                .message_disappear_in_ns(
                    opts.message_disappearing_settings.as_ref().map(|m| m.in_ns),
                )
                .dm_id(Some(
                    DmMembers {
                        member_one_inbox_id: dm_target_inbox_id,
                        member_two_inbox_id: context.identity().inbox_id().to_string(),
                    }
                    .to_string(),
                ))
                .build()?;

            stored_group.store(&db)?;
            Ok::<_, GroupError>(Continue((stored_group, true)))
        })?
        .into_continued();
        let new_group = Self::new_from_arc(
            context.clone(),
            stored_group.id,
            stored_group.dm_id,
            ConversationType::Dm,
            stored_group.created_at_ns,
        );
        // Consent state defaults to allowed when the user creates the group
        if created {
            new_group.update_consent_state(ConsentState::Allowed)?;
        }
        Ok(new_group)
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
