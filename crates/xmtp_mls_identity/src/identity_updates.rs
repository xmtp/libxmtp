use crate::identity_ext::IdentityExt;
use futures::{StreamExt, TryStreamExt, stream};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use thiserror::Error;
use xmtp_common::{ErrorCode, Event, Retry, RetryableError, retry_async, retryable};
use xmtp_configuration::{IDENTITY_DEPENDENCY_CONCURRENCY, MAX_IDENTITY_UPDATE_ACTIONS};
use xmtp_cryptography::CredentialSign;
use xmtp_cryptography::signature::IdentifierValidationError;
use xmtp_db::StorageError;
use xmtp_db::XmtpDb;
use xmtp_db::prelude::*;
use xmtp_db::{db_connection::DbConnection, identity_update::StoredIdentityUpdate};
use xmtp_id::{
    AsIdRef, BlockNumber, Bytes, InboxIdRef,
    associations::{
        AccountId, AssociationError, AssociationState, Handle, Identifier, IdentityAction,
        InstallationKeyContext, MemberIdentifier, SignatureError,
        builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
        unverified::{
            UnverifiedIdentityUpdate, UnverifiedInstallationKeySignature, UnverifiedSignature,
        },
        verify_updates,
    },
    scw_verifier::{SmartContractSignatureVerifier, ValidationResponse, VerifierError},
};
use xmtp_macro::log_event;
use xmtp_mls_core::context::CoreContext;
use xmtp_mls_core::identity::IdentityError;
use xmtp_mls_validation::group_membership::{GroupMembership, MembershipDiff};
use xmtp_proto::api_client::XmtpApi;
use xmtp_proto::{
    ShortHex,
    api_client::XmtpBackendClient,
    types::{Cursor, GroupId, IncomingBatchLimits},
};

use xmtp_api::{ApiClientWrapper, GetIdentityUpdatesV2Filter};

pub mod dependencies;
#[cfg(test)]
pub(crate) mod paged_load_tests;
mod state_store;
#[cfg(any(test, feature = "test-utils"))]
#[doc(hidden)]
pub mod test_support;
pub use dependencies::{IdentityDependencyError, IdentityRequirement, IdentityResolutionRegistry};

mod error;
mod installation_diff;
mod loading;
mod verification;

pub use error::{IdentityUpdateError, InstallationDiffError};
pub use installation_diff::{
    InstallationDiff, get_installation_diff_local, required_leaf_removals,
};
pub use loading::{load_identity_updates, load_identity_updates_for_client};
use verification::check_revocation_count;
pub(crate) use verification::state_holds_identifier;
pub use verification::{
    apply_signature_request_with_verifier, batch_get_association_state_with_verifier,
    get_association_state_with_verifier, get_creation_signature_kind, identifier_membership,
    identifier_opens_inbox, is_member_of_association_state, revoke_installations_with_verifier,
    stored_identifier_membership,
};

/// Identity's feature slot: the registry that coalesces exact identity lookups.
pub trait IdentityFeatures {
    fn identity_resolutions(&self) -> &IdentityResolutionRegistry;
}

/// A context the identity code runs on: core and identity resolution.
pub trait IdentityContext: CoreContext<Features: IdentityFeatures> {
    /// Coalesces exact identity lookups without treating a newer snapshot as the requested one.
    fn identity_resolution_registry(&self) -> &IdentityResolutionRegistry {
        self.features().identity_resolutions()
    }
}

impl<T: CoreContext<Features: IdentityFeatures>> IdentityContext for T {}
pub use dependencies::{
    MissingProofs, OwnInstallationEvents, require_association_state, resolve_identity_requirements,
};
use state_store::{ResolvedState, resolve_association_state};
pub use state_store::{get_association_state_for_client, store_association_state};

const IDENTITY_UPDATE_CONFLICT_RETRIES: usize = 3;

/// Reload and validate the signed update after a commit-time conflict.
/// Retry the same signed bytes at most three times. Other errors are terminal.
pub async fn publish_with_conflict_retry<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &impl DbQuery,
    update: UnverifiedIdentityUpdate,
    verifier: &impl SmartContractSignatureVerifier,
) -> Result<Cursor, IdentityUpdateError> {
    for attempt in 0..=IDENTITY_UPDATE_CONFLICT_RETRIES {
        match api_client.publish_identity_update(update.clone()).await {
            Ok(cursor) => return Ok(cursor),
            Err(xmtp_api::ApiError::IdentityUpdateConflict)
                if attempt < IDENTITY_UPDATE_CONFLICT_RETRIES =>
            {
                load_identity_updates(api_client, conn, &[update.inbox_id.as_str()]).await?;
                let history = conn.get_identity_updates(&update.inbox_id, None, None)?;
                let history = history
                    .into_iter()
                    .map(|stored| stored.to_unverified().map(Into::into))
                    .collect::<Result<Vec<_>, _>>()?;
                xmtp_mls_validation::validate_identity_updates(
                    history,
                    vec![update.clone().into()],
                    verifier,
                )
                .await?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    unreachable!("the final publish attempt returns its result")
}

pub struct IdentityUpdates<Context> {
    context: Context,
}

impl<Context> IdentityUpdates<Context> {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

impl<'a, Context> IdentityUpdates<Context>
where
    Context: IdentityContext,
{
    /// Get the association state for all provided `inbox_id`/optional `sequence_id` tuples, using the cache when available
    /// If the association state is not available in the cache, this falls back to reconstructing the association state
    /// from Identity Updates in the network.
    ///
    /// At most [`IDENTITY_DEPENDENCY_CONCURRENCY`] states are derived at once,
    /// and the states are returned in the order of `identifiers`.
    pub async fn batch_get_association_state(
        &self,
        conn: &impl DbQuery,
        identifiers: &[(impl AsIdRef, Option<i64>)],
    ) -> Result<Vec<AssociationState>, IdentityUpdateError> {
        // Collected first for the reason given in
        // `batch_get_association_state_with_verifier`.
        let reads: Vec<_> = identifiers
            .iter()
            .map(|(inbox_id, to_sequence_id)| {
                get_association_state_for_client(
                    &self.context,
                    conn,
                    inbox_id.as_ref(),
                    *to_sequence_id,
                )
            })
            .collect();
        stream::iter(reads)
            .buffered(IDENTITY_DEPENDENCY_CONCURRENCY)
            .try_collect()
            .await
    }

    /// Get the latest association state available on the network for the given `inbox_id`
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_latest_association_state(
        &self,
        conn: &DbConnection<<Context::Db as XmtpDb>::Connection>,
        inbox_id: InboxIdRef<'a>,
    ) -> Result<AssociationState, IdentityUpdateError> {
        load_identity_updates_for_client(&self.context, conn, &[inbox_id]).await?;
        self.get_association_state(conn, inbox_id, None).await
    }

    /// Emit the installation changes of one own identity update. When it
    /// revokes another installation, cycle the root key. The
    /// revoked installation does not cycle on its own revocation.
    fn emit_own_installation_changes(
        &self,
        conn: &impl DbQuery,
        before: &AssociationState,
        after: &AssociationState,
        revoked_at_ns: i64,
    ) -> Result<(), IdentityUpdateError> {
        use xmtp_db::consent_record::Owed;
        use xmtp_db::user_preferences::{RootKeyWrite, write_root_key};
        use xmtp_events::{ClientEvent, EventWriter, InstallationRef, InstallationRevoked};
        let old: HashSet<_> = before.installation_ids().into_iter().collect();
        let new: HashSet<_> = after.installation_ids().into_iter().collect();
        for installation_key in new.difference(&old) {
            self.context.events().emit(
                Some(ClientEvent::IdentityOwnInstallationAdded(InstallationRef {
                    installation_key: installation_key.clone(),
                })),
                None,
            );
        }
        for installation_key in old.difference(&new) {
            self.context.events().emit(
                Some(ClientEvent::IdentityOwnInstallationRevoked(
                    InstallationRevoked {
                        installation_key: installation_key.clone(),
                        is_this_installation: installation_key.as_slice()
                            == self.context.installation_id().to_vec().as_slice(),
                    },
                )),
                None,
            );
        }
        // implements: SYNC-026
        let me = self.context.installation_id().to_vec();
        if new.contains(&me)
            && old.difference(&new).next().is_some()
            && write_root_key(conn, RootKeyWrite::revocation(revoked_at_ns), Owed::Mark)?
        {
            xmtp_mls_core::events::emit_root_key_changed(self.context.events(), conn, true)?;
        }
        Ok(())
    }

    /// Run `load`, which can store updates of this client's own inbox, with
    /// own-inbox loads serialized. Then, whether `load` succeeded or failed,
    /// emit the own-installation changes of every own-inbox update stored
    /// since the last emission. A load that stores pages and then fails,
    /// including the reload after a publish conflict, therefore still reports
    /// what it stored. Updates whose emission does not happen, because `load`
    /// was dropped or the emission failed, are emitted after the next
    /// own-inbox load. `load` is boxed to keep callers' futures small: SDK
    /// worker futures have a size limit.
    pub async fn with_own_installation_events<T>(
        &self,
        conn: &impl DbQuery,
        load: Pin<Box<impl Future<Output = Result<T, IdentityUpdateError>>>>,
    ) -> Result<T, IdentityUpdateError> {
        let mut events = self
            .context
            .identity_resolution_registry()
            .own_inbox_refresh()
            .lock()
            .await;
        if events.emitted_through.is_none() {
            events.emitted_through = self.latest_own_sequence(conn)?;
        }
        let loaded = load.await;
        let emitted = self.emit_own_installation_events(conn, &mut events).await;
        match (loaded, emitted) {
            (Ok(value), emitted) => emitted.map(|()| value),
            (Err(error), Ok(())) => Err(error),
            (Err(error), Err(emit_error)) => {
                tracing::warn!(
                    %emit_error,
                    "own-installation events wait for the next own-inbox load"
                );
                Err(error)
            }
        }
    }

    fn latest_own_sequence(&self, conn: &impl DbQuery) -> Result<Option<i64>, IdentityUpdateError> {
        let inbox_id = self.context.inbox_id();
        Ok(conn
            .get_latest_sequence_id(&[inbox_id])?
            .get(inbox_id)
            .copied())
    }

    /// Emit the changes of each own-inbox update stored after
    /// `emitted_through`, in sequence order, and move `emitted_through` past
    /// each one emitted. With nothing emitted through yet, the latest stored
    /// update becomes the starting point and nothing is emitted.
    async fn emit_own_installation_events(
        &self,
        conn: &impl DbQuery,
        events: &mut OwnInstallationEvents,
    ) -> Result<(), IdentityUpdateError> {
        let inbox_id = self.context.inbox_id();
        let Some(through) = events.emitted_through else {
            events.emitted_through = self.latest_own_sequence(conn)?;
            return Ok(());
        };
        let updates = conn.get_identity_updates(inbox_id, Some(through), None)?;
        if updates.is_empty() {
            return Ok(());
        }
        let mut state = self
            .get_association_state(conn, inbox_id, Some(through))
            .await?;
        for update in updates {
            let next = self
                .get_association_state(conn, inbox_id, Some(update.sequence_id))
                .await?;
            self.emit_own_installation_changes(conn, &state, &next, update.server_timestamp_ns)?;
            events.emitted_through = Some(update.sequence_id);
            state = next;
        }
        Ok(())
    }

    /// Get the association state for a given inbox_id up to the (and inclusive of) the `to_sequence_id`
    /// If no `to_sequence_id` is provided, use the latest value in the database
    pub async fn get_association_state(
        &self,
        conn: &impl xmtp_db::DbQuery,
        inbox_id: InboxIdRef<'_>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, IdentityUpdateError> {
        get_association_state_for_client(&self.context, conn, inbox_id, to_sequence_id).await
    }

    /// Generate a `CreateInbox` signature request for the given wallet address.
    /// If no nonce is provided, use 0
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn create_inbox(
        &self,
        identifier: Identifier,
        maybe_nonce: Option<u64>,
    ) -> Result<SignatureRequest, IdentityUpdateError> {
        let nonce = maybe_nonce.unwrap_or(0);
        let inbox_id = identifier.inbox_id(nonce)?;
        let installation_public_key = self.context.identity().installation_keys.verifying_key();

        let builder = SignatureRequestBuilder::new(inbox_id);
        let mut signature_request = builder
            .create_inbox(identifier.clone(), nonce)
            .add_association(
                MemberIdentifier::installation(installation_public_key.as_bytes().to_vec()),
                identifier.into(),
            )
            .build();

        let sig_bytes = self
            .context
            .identity()
            .sign_identity_update(signature_request.signature_text())?
            .to_vec();
        // We can pre-sign the request with an installation key signature, since we have access to the key
        signature_request
            .add_signature(
                UnverifiedSignature::InstallationKey(UnverifiedInstallationKeySignature::new(
                    sig_bytes,
                    installation_public_key,
                )),
                &self.context.scw_verifier(),
            )
            .await?;

        // The app fills this request in, so bind it to
        // the chains the deployment accepts before it leaves the client.
        self.context
            .server_configuration()
            .restrict(&mut signature_request);

        Ok(signature_request)
    }

    /// Generate a `AssociateWallet` signature request using an existing wallet and a new wallet address
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn associate_identity(
        &self,
        new_identifier: Identifier,
    ) -> Result<SignatureRequest, IdentityUpdateError> {
        tracing::info!(
            "Associating new wallet with inbox_id {}",
            self.context.inbox_id()
        );
        let inbox_id = self.context.inbox_id();
        let builder = SignatureRequestBuilder::new(inbox_id);
        let installation_public_key = self.context.identity().installation_keys.verifying_key();

        let mut signature_request = builder
            .add_association(new_identifier.into(), installation_public_key.into())
            .build();

        let signature = self
            .context
            .identity()
            .installation_keys
            .credential_sign::<InstallationKeyContext>(signature_request.signature_text())?;

        signature_request
            .add_signature(
                UnverifiedSignature::new_installation_key(signature, installation_public_key),
                &self.context.scw_verifier(),
            )
            .await?;

        // the app fills this request in, so bind it to
        // the chains the deployment accepts before it leaves the client.
        self.context
            .server_configuration()
            .restrict(&mut signature_request);

        Ok(signature_request)
    }

    /// Generate a `ClaimHandle` signature request for `handle`, signed by this
    /// installation's key.
    pub async fn claim_handle_request(
        &self,
        handle: Handle,
    ) -> Result<SignatureRequest, IdentityUpdateError> {
        let installation_keys = &self.context.identity().installation_keys;
        let installation_public_key = installation_keys.verifying_key();
        let mut signature_request = SignatureRequestBuilder::new(self.context.inbox_id())
            .claim_handle(handle, installation_public_key.into())
            .build();
        let signature = installation_keys
            .credential_sign::<InstallationKeyContext>(signature_request.signature_text())?;
        signature_request
            .add_signature(
                UnverifiedSignature::new_installation_key(signature, installation_public_key),
                &self.context.scw_verifier(),
            )
            .await?;
        Ok(signature_request)
    }

    /// Revoke the given identities from the association state for the client's inbox.
    /// More identities than one identity update may revoke fail before any network read.
    pub async fn revoke_identities(
        &self,
        identities_to_revoke: Vec<Identifier>,
    ) -> Result<SignatureRequest, IdentityUpdateError> {
        check_revocation_count(identities_to_revoke.len())?;
        let inbox_id = self.context.inbox_id();
        let current_state = retry_async!(
            Retry::default(),
            (async {
                self.get_association_state(&self.context.db(), inbox_id, None)
                    .await
            })
        )?;
        let mut builder = SignatureRequestBuilder::new(inbox_id);

        for ident in identities_to_revoke {
            builder = builder.revoke_association(
                current_state.recovery_identifier().clone().into(),
                ident.into(),
            )
        }

        let mut signature_request = builder.build();
        self.context
            .server_configuration()
            .restrict(&mut signature_request);
        Ok(signature_request)
    }

    /// Revoke the given installations from the association state for the client's inbox
    pub async fn revoke_installations(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<SignatureRequest, IdentityUpdateError> {
        let inbox_id = self.context.inbox_id();
        let current_state = retry_async!(
            Retry::default(),
            (async {
                self.get_association_state(&self.context.db(), inbox_id, None)
                    .await
            })
        )?;

        let mut result = revoke_installations_with_verifier(
            &current_state.recovery_identifier().clone(),
            inbox_id,
            installation_ids,
        )?;
        // Every request this client hands back is bound to the
        // deployment's accepted chains, so an app cannot sign it from a chain
        // the deployment refuses.
        self.context.server_configuration().restrict(&mut result);

        Ok(result)
    }

    /// Generate a `ChangeRecoveryAddress` signature request using a new identifier
    pub async fn change_recovery_identifier(
        &self,
        new_recovery_identifier: Identifier,
    ) -> Result<SignatureRequest, IdentityUpdateError> {
        let inbox_id = self.context.inbox_id();
        let current_state = retry_async!(
            Retry::default(),
            (async {
                self.get_association_state(&self.context.db(), inbox_id, None)
                    .await
            })
        )?;
        let mut builder = SignatureRequestBuilder::new(inbox_id);
        let member_identifier: MemberIdentifier =
            current_state.recovery_identifier().clone().into();
        builder = builder.change_recovery_address(member_identifier, new_recovery_identifier);
        let mut signature_request = builder.build();
        self.context
            .server_configuration()
            .restrict(&mut signature_request);
        Ok(signature_request)
    }

    /**
     * Apply a signature request to the client's inbox by publishing the identity update to the network.
     *
     * This will error if the signature request is missing signatures, if the signatures are invalid,
     * if the update fails other verifications, or if the update fails to be published to the network.
     **/
    pub async fn apply_signature_request(
        &self,
        signature_request: SignatureRequest,
    ) -> Result<(), IdentityUpdateError> {
        // A client with a blocked connection publishes no identity update.
        self.context.server_configuration().check()?;
        let inbox_id = signature_request.inbox_id().to_string();
        let apply = async {
            apply_signature_request_with_verifier(
                self.context.api(),
                &self.context.db(),
                signature_request,
                &self.context.scw_verifier(),
            )
            .await?;
            // Load the identity updates for the inbox so that we have a
            // record in our DB.
            retry_async!(
                Retry::default(),
                (async {
                    load_identity_updates(
                        self.context.api(),
                        &self.context.db(),
                        &[inbox_id.as_str()],
                    )
                    .await
                })
            )
        };
        if inbox_id == self.context.inbox_id() {
            self.with_own_installation_events(&self.context.db(), Box::pin(apply))
                .await
        } else {
            apply.await
        }
    }

    /// Given two group memberships and the diff, get the list of installations that were added or removed
    /// between the two membership states.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_installation_diff(
        &self,
        conn: &impl DbQuery,
        group_id: &GroupId, // used for logging
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        let diff = loop {
            match get_installation_diff_local(
                conn,
                old_group_membership,
                new_group_membership,
                membership_diff,
            ) {
                Ok(diff) => break diff,
                Err(IdentityDependencyError::Need(requirements)) => {
                    resolve_identity_requirements(&self.context, &requirements).await?;
                }
                Err(error) => return Err(error.into()),
            }
        };
        let InstallationDiff {
            added_installations,
            removed_installations,
        } = diff;

        if !added_installations.is_empty() || !removed_installations.is_empty() {
            let added_installations: Vec<_> =
                added_installations.keys().map(|i| i.short_hex()).collect();
            let removed_installations: Vec<_> = removed_installations
                .iter()
                .map(|i| i.short_hex())
                .collect();
            log_event!(
                Event::UpdatedGroupMembership,
                self.context.installation_id(),
                #group_id,
                ?added_installations,
                ?removed_installations
            );
        }

        Ok(InstallationDiff {
            added_installations,
            removed_installations,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::identity_updates::IdentityUpdates;
    use xmtp_db::prelude::*;
    use xmtp_id::associations::test_utils::add_wallet_signature;

    // verifies: GMOD-046
    #[xmtp_common::test(unwrap_try = true)]
    async fn required_leaf_removals_include_stale_leaves() {
        use super::{
            IdentityDependencyError, IdentityRequirement, get_installation_diff_local,
            required_leaf_removals,
        };
        use crate::identity_lifecycle::test_support::IdentityTestPeer;
        use std::collections::{BTreeSet, HashMap, HashSet};
        use xmtp_mls_validation::group_membership::GroupMembership;
        let alix = IdentityTestPeer::new().await;
        let alix2 = alix.new_installation().await;
        let mut revoke = IdentityUpdates::new(&alix.context)
            .revoke_installations(vec![alix2.installation_public_key().to_vec()])
            .await?;
        add_wallet_signature(&mut revoke, &alix.owner).await;
        IdentityUpdates::new(&alix.context)
            .apply_signature_request(revoke)
            .await?;
        IdentityUpdates::new(&alix.context)
            .get_latest_association_state(&alix.context.db(), alix.inbox_id())
            .await?;
        let sequence2 = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;
        let alix3 = alix.new_installation().await;
        IdentityUpdates::new(&alix.context)
            .get_latest_association_state(&alix.context.db(), alix.inbox_id())
            .await?;
        let sequence3 = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;
        let mut old = GroupMembership::new();
        old.add(alix.inbox_id().into(), sequence2);
        let mut new = GroupMembership::new();
        new.add(alix.inbox_id().into(), sequence3);
        let owners = HashMap::from([
            (
                alix.installation_public_key().to_vec(),
                alix.inbox_id().into(),
            ),
            (
                alix2.installation_public_key().to_vec(),
                alix.inbox_id().into(),
            ),
        ]);
        // Alix2 was created before S3 and has not loaded that proof. A
        // missing proof must not produce an incomplete set of removals.
        let missing =
            required_leaf_removals(&alix2.context.db(), &old, &new, &HashSet::new(), &owners)
                .expect_err("the exact S3 proof is not cached on Alix2");
        assert!(matches!(
            missing,
            IdentityDependencyError::Need(needed)
                if needed == BTreeSet::from([IdentityRequirement {
                    inbox_id: alix.inbox_id().into(),
                    sequence_id: sequence3,
                }])
        ));

        let db = alix.context.db();
        let diff = get_installation_diff_local(&db, &old, &new, &old.diff(&new))?;
        assert!(diff.removed_installations.is_empty());
        let stale = HashSet::from([alix2.installation_public_key().to_vec()]);
        assert_eq!(
            required_leaf_removals(&db, &old, &new, &diff.removed_installations, &owners)?,
            stale
        );
        assert_eq!(
            required_leaf_removals(&db, &old, &GroupMembership::new(), &HashSet::new(), &owners)?,
            owners.keys().cloned().collect()
        );
        old.add(alix.inbox_id().into(), 0);
        assert_eq!(
            required_leaf_removals(&db, &old, &new, &HashSet::new(), &owners)?,
            stale
        );
        assert!(required_leaf_removals(&db, &new, &new, &HashSet::new(), &owners)?.is_empty());
        drop(alix3);
    }
}

#[cfg(test)]
mod conflict_tests {
    use super::*;
    use crate::identity_lifecycle::test_support::IdentityTestPeer;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::test_utils::{
        MockSmartContractSignatureVerifier, WalletTestExt, add_wallet_signature,
    };
    use xmtp_proto::{api::ApiClientError, backend_v1 as wire, types::Topic};

    #[rstest::rstest]
    #[case(0)]
    #[case(1)]
    #[case(3)]
    #[case(4)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn conflict_reloads_validates_and_bounds_identical_resends(#[case] conflicts: usize) {
        let alix = IdentityTestPeer::new().await;
        let wallet = generate_local_wallet();
        let mut request = IdentityUpdates::new(&alix.context)
            .associate_identity(wallet.identifier())
            .await
            .unwrap();
        add_wallet_signature(&mut request, &wallet).await;
        let update = request.build_identity_update().unwrap();
        let expected: xmtp_proto::xmtp::identity::associations::IdentityUpdate =
            update.clone().into();
        let history = alix
            .context
            .api()
            .query_all(
                [(
                    Topic::new_identity_update(hex::decode(alix.inbox_id()).unwrap()),
                    Cursor(0),
                )]
                .into(),
                alix.context.api().limits().max_query_limit as u32,
            )
            .await
            .unwrap();
        let last_sequence = history
            .last()
            .unwrap()
            .meta
            .as_ref()
            .unwrap()
            .cursor
            .as_ref()
            .unwrap()
            .sequence_id;
        let mut mock = xmtp_api_backend::MockBackendClient::new();
        let mut calls = 0;
        mock.expect_publish()
            .times((conflicts + 1).min(4))
            .returning(move |request| {
                calls += 1;
                assert_eq!(request.envelopes.len(), 1);
                assert_eq!(
                    request.envelopes[0].payload,
                    Some(wire::client_envelope::Payload::IdentityUpdate(
                        expected.clone()
                    ))
                );
                if calls <= conflicts {
                    return Err(ApiClientError::client(
                        xmtp_api_grpc::error::GrpcError::Status(tonic::Status::aborted(
                            "identity conflict",
                        )),
                    ));
                }
                let parsed =
                    xmtp_mls_validation::parse_envelope(request.envelopes[0].clone()).unwrap();
                Ok(wire::PublishResponse {
                    envelope_metas: vec![wire::EnvelopeMeta {
                        cursor: Some(wire::Cursor {
                            sequence_id: last_sequence + 1,
                        }),
                        topic: Some(wire::Topic {
                            topic: parsed.topic.cloned_vec(),
                        }),
                        message_hash: Some(wire::MessageHash {
                            hash: Some(wire::message_hash::Hash::Sha256(
                                parsed.canonical.hash.to_vec(),
                            )),
                        }),
                        ..Default::default()
                    }],
                })
            });
        mock.expect_query_raw_envelopes()
            .times(conflicts.min(3))
            .returning(move |request| {
                assert_eq!(request.queries.len(), 1);
                let floor = request.queries[0].cursor.as_ref().unwrap().sequence_id;
                Ok(wire::QueryResponse {
                    envelopes: history
                        .iter()
                        .filter(|entry| {
                            entry
                                .meta
                                .as_ref()
                                .unwrap()
                                .cursor
                                .as_ref()
                                .unwrap()
                                .sequence_id
                                > floor
                        })
                        .cloned()
                        .collect(),
                    continuation: Some(wire::Continuation { has_more: false }),
                }
                .into())
            });
        let api = ApiClientWrapper::new(mock, Default::default());
        let result = publish_with_conflict_retry(
            &api,
            &alix.context.db(),
            update,
            &MockSmartContractSignatureVerifier::new(true),
        )
        .await;
        if conflicts > 3 {
            assert!(matches!(
                result,
                Err(IdentityUpdateError::Api(
                    xmtp_api::ApiError::IdentityUpdateConflict
                ))
            ));
        } else {
            assert_eq!(result.unwrap(), Cursor(last_sequence + 1));
        }
    }
}
