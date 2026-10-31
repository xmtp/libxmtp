use crate::{
    XmtpApi,
    client::ClientError,
    context::XmtpSharedContext,
    groups::group_membership::{GroupMembership, MembershipDiff},
    identity::{IdentityError, IdentityExt},
};
use futures::{StreamExt, TryStreamExt, stream};
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use thiserror::Error;
use xmtp_common::{Event, Retry, RetryableError, retry_async, retryable};
use xmtp_configuration::{IDENTITY_DEPENDENCY_CONCURRENCY, MAX_IDENTITY_UPDATE_ACTIONS};
use xmtp_cryptography::CredentialSign;
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
use xmtp_proto::{
    ShortHex,
    api_client::XmtpBackendClient,
    types::{Cursor, GroupId, IncomingBatchLimits},
};

use xmtp_api::{ApiClientWrapper, GetIdentityUpdatesV2Filter};

mod dependencies;
#[cfg(test)]
mod paged_load_tests;
mod state_store;
pub use dependencies::{IdentityDependencyError, IdentityRequirement, IdentityResolutionRegistry};
pub(crate) use dependencies::{
    MissingProofs, OwnInstallationEvents, require_association_state, resolve_identity_requirements,
};
use state_store::{ResolvedState, resolve_association_state};
pub(crate) use state_store::{get_association_state_for_client, store_association_state};

#[derive(Debug, Error)]
pub enum IdentityUpdateError {
    #[error(transparent)]
    InvalidSignatureRequest(#[from] SignatureRequestError),
    #[error(transparent)]
    Api(#[from] xmtp_api::ApiError),
    #[error(transparent)]
    Validation(#[from] xmtp_mls_validation::ValidationError),
    #[error(transparent)]
    Load(Box<ClientError>),
}

const IDENTITY_UPDATE_CONFLICT_RETRIES: usize = 3;

/// Reload and validate the signed update after a commit-time conflict.
/// Retry the same signed bytes at most three times. Other errors are terminal.
pub(crate) async fn publish_with_conflict_retry<ApiClient: XmtpApi>(
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
                load_identity_updates(api_client, conn, &[update.inbox_id.as_str()])
                    .await
                    .map_err(|error| IdentityUpdateError::Load(Box::new(error)))?;
                let history = conn
                    .get_identity_updates(&update.inbox_id, None, None)
                    .map_err(|error| IdentityUpdateError::Load(Box::new(error.into())))?;
                let history = history
                    .into_iter()
                    .map(|stored| stored.to_unverified().map(Into::into))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| IdentityUpdateError::Load(Box::new(error.into())))?;
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

#[derive(Debug)]
pub struct InstallationDiff {
    /// Each added installation key, mapped to the inboxes whose identity states add it.
    pub added_installations: HashMap<Vec<u8>, HashSet<String>>,
    pub removed_installations: HashSet<Vec<u8>>,
}

#[derive(Debug, Error)]
pub enum InstallationDiffError {
    #[error(transparent)]
    IdentityDependency(#[from] IdentityDependencyError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error(transparent)]
    Db(#[from] xmtp_db::ConnectionError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl RetryableError for InstallationDiffError {
    fn is_retryable(&self) -> bool {
        match self {
            InstallationDiffError::IdentityDependency(error) => retryable!(error),
            InstallationDiffError::Client(client_error) => retryable!(client_error),
            InstallationDiffError::Storage(e) => retryable!(e),
            InstallationDiffError::Db(e) => retryable!(e),
        }
    }

    fn rate_limit_delay(&self) -> Option<xmtp_common::time::Duration> {
        match self {
            InstallationDiffError::IdentityDependency(error) => error.rate_limit_delay(),
            InstallationDiffError::Client(client_error) => client_error.rate_limit_delay(),
            _ => None,
        }
    }
}

pub struct IdentityUpdates<Context> {
    context: Context,
}

impl<Context> IdentityUpdates<Context> {
    pub fn new(context: Context) -> Self {
        Self { context }
    }
}

/// Get the association state for a given inbox_id up to the (and inclusive of) the `to_sequence_id`
/// If no `to_sequence_id` is provided, use the latest value in the database
///
/// This path has no client, so storing a computed state emits no event. A
/// client stores through [`IdentityUpdates::get_association_state`].
pub async fn get_association_state_with_verifier(
    conn: &impl DbQuery,
    inbox_id: &str,
    to_sequence_id: Option<i64>,
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<AssociationState, ClientError> {
    match resolve_association_state(conn, inbox_id, to_sequence_id, scw_verifier).await? {
        ResolvedState::Cached(state) => Ok(state),
        ResolvedState::Computed { sequence_id, state } => {
            conn.write_to_cache(inbox_id.to_owned(), sequence_id, state.clone().into())?;
            Ok(state)
        }
    }
}

/// Whether `state` holds `identifier`. An Ethereum address matches in any
/// letter case, and a passkey by its public key bytes.
pub(crate) fn state_holds_identifier(state: &AssociationState, identifier: &Identifier) -> bool {
    identifier
        .clone()
        .sanitize()
        .is_ok_and(|identifier| state.get(&identifier.into()).is_some())
}

/// Whether the association state of the inbox in the database holds the
/// identifier, an Ethereum address in any letter case, or `None` when the
/// database has no identity update of the inbox. Computing the state caches
/// it in the database.
pub async fn identifier_membership(
    conn: &impl DbQuery,
    inbox_id: &str,
    identifier: &Identifier,
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<Option<bool>, ClientError> {
    match get_association_state_with_verifier(conn, inbox_id, None, scw_verifier).await {
        Ok(state) => Ok(Some(state_holds_identifier(&state, identifier))),
        Err(ClientError::Association(AssociationError::MissingIdentityUpdate)) => Ok(None),
        Err(error) => Err(error),
    }
}

/// [`identifier_membership`] for a caller that may make no request, such as an
/// offline build. It reads only the database: the cached state, or one
/// derived from the stored log. A state whose derivation needs a smart
/// contract wallet verdict counts as not holding the identifier.
pub async fn stored_identifier_membership(
    conn: &impl DbQuery,
    inbox_id: &str,
    identifier: &Identifier,
) -> Result<Option<bool>, ClientError> {
    match identifier_membership(conn, inbox_id, identifier, &NoRequestVerifier).await {
        Err(ClientError::SignatureValidation(SignatureError::VerifierError(_))) => Ok(Some(false)),
        membership => membership,
    }
}

/// A verifier that makes no request, so it fails every smart contract wallet
/// check.
struct NoRequestVerifier;

#[xmtp_common::async_trait]
impl SmartContractSignatureVerifier for NoRequestVerifier {
    async fn is_valid_signature(
        &self,
        _account_id: AccountId,
        _hash: [u8; 32],
        _signature: Bytes,
        _block_number: Option<BlockNumber>,
    ) -> Result<ValidationResponse, VerifierError> {
        Err(std::io::Error::other(
            "a check that makes no request verifies no smart contract wallet",
        )
        .into())
    }
}

/// Whether the identifier may open the database of the inbox, given its
/// [`identifier_membership`]. Only the identity that creates an inbox belongs
/// to it before its first identity update.
pub fn identifier_opens_inbox(
    membership: Option<bool>,
    identifier: &Identifier,
    nonce: u64,
    inbox_id: &str,
) -> bool {
    membership.unwrap_or_else(|| identifier.inbox_id(nonce).is_ok_and(|own| own == inbox_id))
}

/// Refuse to revoke more members than one identity update may carry, before
/// any request is built or signed: every validator would reject that update.
fn check_revocation_count(count: usize) -> Result<(), ClientError> {
    if count > MAX_IDENTITY_UPDATE_ACTIONS {
        return Err(SignatureRequestError::TooManyActions(count).into());
    }
    Ok(())
}

/// Revoke the given installations from the association state for the client's inbox.
/// More installations than one identity update may revoke fail before a request is built.
pub fn revoke_installations_with_verifier(
    identifier: &Identifier,
    inbox_id: &str,
    installation_ids: Vec<Vec<u8>>,
) -> Result<SignatureRequest, ClientError> {
    check_revocation_count(installation_ids.len())?;
    let mut builder = SignatureRequestBuilder::new(inbox_id);

    for installation_id in installation_ids {
        builder = builder.revoke_association(
            identifier.clone().into(),
            MemberIdentifier::installation(installation_id),
        )
    }

    Ok(builder.build())
}

/**
 * Apply a signature request to the client's inbox by publishing the identity update to the network.
 *
 * This will error if the signature request is missing signatures, if the signatures are invalid,
 * if the update fails other verifications, or if the update fails to be published to the network.
 **/
pub async fn apply_signature_request_with_verifier<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &impl DbQuery,
    signature_request: SignatureRequest,
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<(), ClientError> {
    // If the signature request isn't completed, this will error
    let identity_update = signature_request
        .build_identity_update()
        .map_err(IdentityUpdateError::from)?;

    identity_update.to_verified(scw_verifier).await?;

    publish_with_conflict_retry(api_client, conn, identity_update, scw_verifier).await?;

    Ok(())
}

/// Get the association state for all provided `inbox_id`/optional `sequence_id` tuples, using the cache when available
/// If the association state is not available in the cache, this falls back to reconstructing the association state
/// from Identity Updates in the network.
///
/// At most [`IDENTITY_DEPENDENCY_CONCURRENCY`] states are derived at once, and
/// the states are returned in the order of `identifiers`.
pub async fn batch_get_association_state_with_verifier(
    conn: &impl DbQuery,
    identifiers: &[(impl AsIdRef, Option<i64>)],
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<Vec<AssociationState>, ClientError> {
    // Each read is a future that starts only when `buffered` polls it. They
    // are collected first: a stream that maps the borrowed identifiers is not
    // `Send` for every lifetime that callers such as the SDK need.
    let reads: Vec<_> = identifiers
        .iter()
        .map(|(inbox_id, to_sequence_id)| {
            get_association_state_with_verifier(
                conn,
                inbox_id.as_ref(),
                *to_sequence_id,
                scw_verifier,
            )
        })
        .collect();
    stream::iter(reads)
        .buffered(IDENTITY_DEPENDENCY_CONCURRENCY)
        .try_collect()
        .await
}

impl<'a, Context> IdentityUpdates<Context>
where
    Context: XmtpSharedContext,
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
    ) -> Result<Vec<AssociationState>, ClientError> {
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
    ) -> Result<AssociationState, ClientError> {
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
    ) -> Result<(), ClientError> {
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
            crate::subscriptions::internal::emit_root_key_changed(
                self.context.events(),
                conn,
                true,
            )?;
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
    pub(crate) async fn with_own_installation_events<T>(
        &self,
        conn: &impl DbQuery,
        load: Pin<Box<impl Future<Output = Result<T, ClientError>>>>,
    ) -> Result<T, ClientError> {
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

    fn latest_own_sequence(&self, conn: &impl DbQuery) -> Result<Option<i64>, ClientError> {
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
    ) -> Result<(), ClientError> {
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
    ) -> Result<AssociationState, ClientError> {
        get_association_state_for_client(&self.context, conn, inbox_id, to_sequence_id).await
    }

    /// Generate a `CreateInbox` signature request for the given wallet address.
    /// If no nonce is provided, use 0
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn create_inbox(
        &self,
        identifier: Identifier,
        maybe_nonce: Option<u64>,
    ) -> Result<SignatureRequest, ClientError> {
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
    ) -> Result<SignatureRequest, ClientError> {
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
    ) -> Result<SignatureRequest, ClientError> {
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
    ) -> Result<SignatureRequest, ClientError> {
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
    ) -> Result<SignatureRequest, ClientError> {
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
    ) -> Result<SignatureRequest, ClientError> {
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
    ) -> Result<(), ClientError> {
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

/// Compare exact verified snapshots without network or signature-verifier calls.
/// A creation placeholder has an empty identity baseline. It never selects the
/// receiver's latest identity state. New membership references must be nonzero.
/// A missing proof does not stop the scan, so `Need` names every missing proof.
pub(crate) fn get_installation_diff_local(
    conn: &impl DbQuery,
    old_membership: &GroupMembership,
    new_membership: &GroupMembership,
    membership_diff: &MembershipDiff<'_>,
) -> Result<InstallationDiff, IdentityDependencyError> {
    let mut added_installations = HashMap::new();
    let mut removed_installations = HashSet::new();
    let mut missing = MissingProofs::default();
    for inbox_id in membership_diff
        .added_inboxes
        .iter()
        .chain(&membership_diff.updated_inboxes)
    {
        let final_state = missing.read(
            conn,
            &IdentityRequirement {
                inbox_id: (*inbox_id).clone(),
                sequence_id: new_membership.get(inbox_id).copied().unwrap_or(0),
            },
        )?;
        let initial_state = match old_membership.get(inbox_id) {
            None | Some(0) => None,
            Some(sequence_id) => Some(missing.read(
                conn,
                &IdentityRequirement {
                    inbox_id: (*inbox_id).clone(),
                    sequence_id: *sequence_id,
                },
            )?),
        };
        let diff = match (final_state, initial_state) {
            (Some(final_state), None) => final_state.as_diff(),
            (Some(final_state), Some(Some(initial_state))) => initial_state.diff(&final_state),
            (None, _) | (_, Some(None)) => continue,
        };
        for installation in diff.new_installations() {
            added_installations
                .entry(installation)
                .or_insert_with(HashSet::new)
                .insert((*inbox_id).clone());
        }
        removed_installations.extend(diff.removed_installations());
    }
    for inbox_id in &membership_diff.removed_inboxes {
        let Some(state) = missing.read(
            conn,
            &IdentityRequirement {
                inbox_id: (*inbox_id).clone(),
                sequence_id: old_membership.get(inbox_id).copied().unwrap_or(0),
            },
        )?
        else {
            continue;
        };
        removed_installations.extend(state.installation_ids());
    }
    missing.check()?;
    Ok(InstallationDiff {
        added_installations,
        removed_installations,
    })
}

/// Required removals include leafed identity removals and stale leaves of
/// every inbox whose membership entry advances or disappears.
// implements: GMOD-046
pub(crate) fn required_leaf_removals(
    conn: &impl DbQuery,
    old: &GroupMembership,
    new: &GroupMembership,
    diff_removals: &HashSet<Vec<u8>>,
    leaf_owners: &HashMap<Vec<u8>, String>,
) -> Result<HashSet<Vec<u8>>, IdentityDependencyError> {
    let mut required: HashSet<_> = diff_removals
        .iter()
        .filter(|key| leaf_owners.contains_key(*key))
        .cloned()
        .collect();
    let mut by_inbox: HashMap<&str, Vec<&Vec<u8>>> = HashMap::new();
    for (key, inbox) in leaf_owners {
        by_inbox.entry(inbox).or_default().push(key);
    }
    let mut missing = MissingProofs::default();
    for (inbox, keys) in by_inbox {
        match (old.get(inbox), new.get(inbox)) {
            (Some(_), None) => required.extend(keys.into_iter().cloned()),
            (before, Some(after)) if *after != 0 && before != Some(after) => {
                let Some(state) = missing.read(
                    conn,
                    &IdentityRequirement {
                        inbox_id: inbox.to_string(),
                        sequence_id: *after,
                    },
                )?
                else {
                    continue;
                };
                let associated: HashSet<_> = state.installation_ids().into_iter().collect();
                required.extend(
                    keys.into_iter()
                        .filter(|key| !associated.contains(*key))
                        .cloned(),
                );
            }
            _ => {}
        }
    }
    missing.check()?;
    Ok(required)
}

/// Load identity updates. A load that includes this client's own inbox
/// reports the changes to its own installations of every update it stored,
/// even when it fails after storing some pages.
pub(crate) async fn load_identity_updates_for_client<Context: XmtpSharedContext>(
    context: &Context,
    conn: &impl DbQuery,
    inbox_ids: &[&str],
) -> Result<(), ClientError> {
    let load = Box::pin(store_identity_update_pages(
        context.api(),
        conn,
        inbox_ids,
        context.incoming_runtime().policy().fetched_limits(),
    ));
    if inbox_ids.contains(&context.inbox_id()) {
        IdentityUpdates::new(context)
            .with_own_installation_events(conn, load)
            .await
    } else {
        load.await
    }
}

/// Store every identity update of `inbox_ids` newer than the latest stored
/// one of each inbox.
#[tracing::instrument(level = "trace", skip_all)]
pub async fn load_identity_updates<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &impl xmtp_db::DbQuery,
    inbox_ids: &[&str],
) -> Result<(), ClientError> {
    store_identity_update_pages(
        api_client,
        conn,
        inbox_ids,
        IncomingBatchLimits::STREAM_BATCH,
    )
    .await
}

/// Read the updates of `inbox_ids` after the latest stored one of each inbox,
/// one bounded page at a time, and store each page before the next request.
/// A failure leaves each inbox with a stored prefix of its log, and the next
/// load resumes after it.
async fn store_identity_update_pages<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &impl DbQuery,
    inbox_ids: &[&str],
    limits: IncomingBatchLimits,
) -> Result<(), ClientError> {
    if inbox_ids.is_empty() {
        return Ok(());
    }
    tracing::debug!("Fetching identity updates for: {:?}", inbox_ids);

    let existing_sequence_ids = conn.get_latest_sequence_id(inbox_ids)?;
    let filters: Vec<GetIdentityUpdatesV2Filter> = inbox_ids
        .iter()
        .map(|inbox_id| GetIdentityUpdatesV2Filter {
            sequence_id: existing_sequence_ids.get(*inbox_id).map(|i| *i as u64),
            inbox_id: inbox_id.to_string(),
        })
        .collect();
    api_client
        .fold_identity_updates(filters, limits, |page| {
            let page = page
                .into_iter()
                .map(|entry| {
                    let sequence_id = entry
                        .meta
                        .cursor
                        .ok_or(xmtp_api::ApiError::InvalidResponse("identity cursor"))?
                        .sequence_id;
                    let inbox_id = entry.update.inbox_id.clone();
                    let update: UnverifiedIdentityUpdate = entry.update.try_into()?;
                    Ok(StoredIdentityUpdate {
                        inbox_id,
                        sequence_id: sequence_id as i64,
                        server_timestamp_ns: entry.meta.server_ns as i64,
                        payload: update.into(),
                    })
                })
                .collect::<Result<Vec<_>, ClientError>>()?;
            conn.insert_or_ignore_identity_updates(&page)?;
            Ok(())
        })
        .await
}

/// A static lookup method to verify if an identity is a member of an inbox
pub async fn is_member_of_association_state<Client>(
    api_client: &ApiClientWrapper<Client>,
    inbox_id: &str,
    identifier: &MemberIdentifier,
    scw_verifier: Option<Box<dyn SmartContractSignatureVerifier>>,
) -> Result<bool, ClientError>
where
    Client: XmtpBackendClient + Clone,
{
    let filters = vec![GetIdentityUpdatesV2Filter {
        inbox_id: inbox_id.to_string(),
        sequence_id: None,
    }];
    let mut updates = api_client.get_identity_updates_v2(filters).await?;

    let Some(updates) = updates.remove(inbox_id) else {
        return Err(ClientError::Generic(
            "Unable to find provided inbox_id".to_string(),
        ));
    };
    let updates: Vec<UnverifiedIdentityUpdate> = updates
        .into_iter()
        .map(|u| u.update.try_into())
        .collect::<Result<_, _>>()?;

    let mut association_state = None;

    let scw_verifier = scw_verifier
        .unwrap_or_else(|| Box::new(api_client.clone()) as Box<dyn SmartContractSignatureVerifier>);

    let updates = verify_updates(updates, &scw_verifier).await?;

    for update in updates {
        association_state =
            Some(update.update_state(association_state, update.client_timestamp_ns)?);
    }
    let association_state = association_state.ok_or(ClientError::Generic(
        "Unable to create association state".to_string(),
    ))?;

    Ok(association_state.get(identifier).is_some())
}

#[tracing::instrument(level = "trace", skip_all)]
pub async fn get_creation_signature_kind(
    conn: &impl xmtp_db::DbQuery,
    scw_verifier: impl SmartContractSignatureVerifier,
    inbox_id: InboxIdRef<'_>,
) -> Result<Option<xmtp_id::associations::SignatureKind>, ClientError> {
    let updates = conn.get_identity_updates(inbox_id, None, None)?;

    let first_update = updates
        .first()
        .ok_or_else(|| ClientError::Identity(IdentityError::RequiredIdentityNotFound))?;

    let unverified_update: UnverifiedIdentityUpdate = first_update.clone().to_unverified()?;

    let verified = unverified_update.to_verified(scw_verifier).await?;

    Ok(verified.creation_signature_kind())
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(unused)] // b/c wasm & native
    use crate::{
        Client, XmtpApi,
        builder::ClientBuilder,
        context::XmtpSharedContext,
        groups::group_membership::GroupMembership,
        identity_updates::IdentityUpdates,
        tester,
        utils::{FullXmtpClient, Tester},
    };
    // verifies: GMOD-046
    #[xmtp_common::test(unwrap_try = true)]
    async fn required_leaf_removals_include_stale_leaves() {
        use super::{
            IdentityDependencyError, IdentityRequirement, get_installation_diff_local,
            required_leaf_removals,
        };
        use std::collections::{BTreeSet, HashMap, HashSet};
        tester!(alix, disable_workers);
        tester!(alix2, from: alix, disable_workers);
        let mut revoke = alix
            .identity_updates()
            .revoke_installations(vec![alix2.installation_id.to_vec()])
            .await?;
        add_wallet_signature(&mut revoke, &alix.builder.owner).await;
        alix.identity_updates()
            .apply_signature_request(revoke)
            .await?;
        alix.inbox_state(true).await?;
        let sequence2 = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;
        tester!(alix3, from: alix, disable_workers);
        alix.inbox_state(true).await?;
        let sequence3 = alix
            .context
            .db()
            .get_latest_sequence_id_for_inbox(alix.inbox_id())? as u64;
        let mut old = GroupMembership::new();
        old.add(alix.inbox_id().into(), sequence2);
        let mut new = GroupMembership::new();
        new.add(alix.inbox_id().into(), sequence3);
        let owners = HashMap::from([
            (alix.installation_id.to_vec(), alix.inbox_id().into()),
            (alix2.installation_id.to_vec(), alix.inbox_id().into()),
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
        let stale = HashSet::from([alix2.installation_id.to_vec()]);
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

    use alloy::signers::Signer;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::{
        InboxOwner,
        associations::{
            AssociationState, MemberIdentifier,
            builder::{SignatureRequest, SignatureRequestError},
            test_utils::{MockSmartContractSignatureVerifier, WalletTestExt, add_wallet_signature},
            unverified::UnverifiedSignature,
        },
    };

    use xmtp_db::{
        ConnectionExt, db_connection::DbConnection, identity_update::StoredIdentityUpdate,
        prelude::*,
    };
    use xmtp_proto::types::GroupId;

    use xmtp_common::rand_vec;

    use super::{
        batch_get_association_state_with_verifier, is_member_of_association_state,
        load_identity_updates,
    };
    use alloy::primitives::{BlockNumber, Bytes};
    use prost::Message;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use xmtp_id::{
        associations::{
            AccountId, Identifier,
            unsigned_actions::UnsignedCreateInbox,
            unverified::{UnverifiedAction, UnverifiedCreateInbox, UnverifiedIdentityUpdate},
        },
        scw_verifier::{
            ScwInput, SmartContractSignatureVerifier, ValidationResponse, VerifierError,
        },
    };
    use xmtp_proto::xmtp::identity::associations::IdentityUpdate as IdentityUpdateProto;

    async fn get_association_state<Context>(
        client: &Client<Context>,
        inbox_id: &str,
    ) -> AssociationState
    where
        Context: XmtpSharedContext,
    {
        let conn = client.context.db();
        load_identity_updates(client.context.api(), &conn, &[inbox_id])
            .await
            .unwrap();

        IdentityUpdates::new(&client.context)
            .get_association_state(&conn, inbox_id, None)
            .await
            .unwrap()
    }

    fn insert_identity_update<C>(conn: &DbConnection<C>, inbox_id: &str, sequence_id: i64)
    where
        C: ConnectionExt,
    {
        let identity_update =
            StoredIdentityUpdate::new(inbox_id.to_string(), sequence_id, 0, rand_vec::<24>());

        conn.insert_or_ignore_identity_updates(&[identity_update])
            .expect("insert should succeed");
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn test_is_member_of_association_state() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let wallet2 = generate_local_wallet();
        let client_identity_updates = IdentityUpdates::new(&client.context);

        let mut request = client_identity_updates
            .associate_identity(wallet2.identifier())
            .await
            .unwrap();
        add_wallet_signature(&mut request, &wallet2).await;
        client_identity_updates
            .apply_signature_request(request)
            .await
            .unwrap();

        let conn = client.context.db();

        // The installation, wallet1 address, and the newly associated wallet2 address
        // Use wait_for_eq to handle eventual consistency after apply_signature_request
        xmtp_common::wait_for_eq(
            || async {
                client_identity_updates
                    .get_latest_association_state(&conn, client.inbox_id())
                    .await
                    .unwrap()
                    .members()
                    .len()
            },
            3,
        )
        .await
        .unwrap();

        let api_client = client.context.api();

        // Check that the second wallet is associated with our new static helper
        let is_member = is_member_of_association_state(
            api_client,
            client.inbox_id(),
            &wallet2.member_identifier(),
            None,
        )
        .await
        .unwrap();

        assert!(is_member);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn create_inbox_round_trip() {
        let wallet = generate_local_wallet();
        let wallet_ident = wallet.identifier();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let mut signature_request: SignatureRequest = client
            .identity_updates()
            .create_inbox(wallet_ident.clone(), None)
            .await
            .unwrap();
        let inbox_id = signature_request.inbox_id().to_string();

        add_wallet_signature(&mut signature_request, &wallet).await;

        client
            .identity_updates()
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        let association_state = get_association_state(&client, &inbox_id).await;

        assert_eq!(association_state.members().len(), 2);
        assert_eq!(association_state.recovery_identifier(), &wallet_ident);
        assert!(association_state.get(&wallet_ident.into()).is_some())
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn add_association() {
        let wallet_2 = generate_local_wallet();
        let wallet2_ident = wallet_2.identifier();

        tester!(client);

        let mut add_association_request = client
            .identity_updates()
            .associate_identity(wallet2_ident.clone())
            .await
            .unwrap();

        add_wallet_signature(&mut add_association_request, &wallet_2).await;

        client
            .identity_updates()
            .apply_signature_request(add_association_request)
            .await
            .unwrap();
        let association_state = get_association_state(&client, client.inbox_id()).await;

        let members = association_state
            .members_by_parent(&client.builder.owner.get_identifier().unwrap().into());
        // Those members should have timestamps
        for member in members {
            assert!(member.client_timestamp_ns.is_some());
        }

        assert_eq!(association_state.members().len(), 3);
        assert_eq!(
            *association_state.recovery_identifier(),
            client.builder.owner.get_identifier().unwrap()
        );
        assert!(association_state.get(&wallet2_ident.into()).is_some());
    }

    #[cfg_attr(not(target_arch = "wasm32"), test)]
    #[cfg(not(target_arch = "wasm32"))]
    fn cache_association_state() {
        use std::sync::Arc;

        use xmtp_common::{assert_logged, traced_test::LOG_BUFFER};

        use crate::{
            utils::LocalTester, worker::device_sync::DeviceSyncClient,
            worker::metrics::WorkerMetrics,
        };

        xmtp_common::traced_test!(async {
            let client = Tester::new().await;
            let inbox_id = client.inbox_id();
            let metrics = WorkerMetrics::new(client.context.installation_id());
            let device_sync = DeviceSyncClient::new(&client.context, Arc::new(metrics));
            device_sync.wait_for_sync_worker_init().await;

            let wallet_2 = generate_local_wallet();

            get_association_state(&client, inbox_id).await;

            assert_logged!("Loaded association", 0);
            // TODO: Verify state is actually in db instead of just checking logs
            assert_logged!("Wrote association", 1);

            let association_state = get_association_state(&client, inbox_id).await;

            assert_eq!(association_state.members().len(), 2);
            assert_eq!(
                association_state.recovery_identifier(),
                &client.builder.owner.identifier()
            );
            assert!(
                association_state
                    .get(&client.builder.owner.identifier().into())
                    .is_some()
            );

            assert_logged!("Loaded association", 1);
            assert_logged!("Wrote association", 1);

            let mut add_association_request = client
                .identity_updates()
                .associate_identity(wallet_2.identifier())
                .await
                .unwrap();

            add_wallet_signature(&mut add_association_request, &wallet_2).await;

            client
                .identity_updates()
                .apply_signature_request(add_association_request)
                .await
                .unwrap();

            assert_logged!("Wrote association", 2);
            // Count cache reads from the following calls.
            LOG_BUFFER.with(|buf| buf.clear());
            get_association_state(&client, inbox_id).await;

            assert_logged!("Loaded association", 1);
            assert_logged!("Wrote association", 0);

            let association_state = get_association_state(&client, inbox_id).await;

            assert_logged!("Loaded association", 2);
            assert_logged!("Wrote association", 0);

            assert_eq!(association_state.members().len(), 3);
            assert_eq!(
                association_state.recovery_identifier(),
                &client.builder.owner.identifier()
            );
            assert!(
                association_state
                    .get(&wallet_2.member_identifier())
                    .is_some()
            );
        });
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn load_identity_updates_if_needed() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;
        let conn = client.context.db();

        insert_identity_update(&conn, "inbox_1", 1);
        insert_identity_update(&conn, "inbox_2", 2);
        insert_identity_update(&conn, "inbox_3", 3);

        let filtered =
            // Inbox 1 is requesting an inbox ID higher than what is in the DB. Inbox 2 is requesting one that matches the DB.
            // Inbox 3 is requesting one lower than what is in the DB
            crate::groups::filter_inbox_ids_needing_updates(&conn, &[("inbox_1", 3), ("inbox_2", 2), ("inbox_3", 2)]);
        assert_eq!(filtered.unwrap(), vec!["inbox_1"]);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    async fn get_installation_diff() {
        let wallet_1 = generate_local_wallet();
        let wallet_2 = generate_local_wallet();
        let wallet_3 = generate_local_wallet();

        let client_1 = ClientBuilder::new_test_client(&wallet_1).await;
        let client_2 = ClientBuilder::new_test_client(&wallet_2).await;
        let client_3 = ClientBuilder::new_test_client(&wallet_3).await;

        let client_2_installation_key = client_2.installation_public_key().to_vec();
        let client_3_installation_key = client_3.installation_public_key().to_vec();

        let mut inbox_ids: Vec<String> = vec![];

        // Create an inbox with 2 history items for each client
        for (client, wallet) in [
            (client_1, wallet_1),
            (client_2, wallet_2),
            (client_3, wallet_3),
        ] {
            let mut signature_request: SignatureRequest = client
                .identity_updates()
                .create_inbox(wallet.identifier(), None)
                .await
                .unwrap();
            let inbox_id = signature_request.inbox_id().to_string();
            inbox_ids.push(inbox_id);

            add_wallet_signature(&mut signature_request, &wallet).await;
            client
                .identity_updates()
                .apply_signature_request(signature_request)
                .await
                .unwrap();
            let new_wallet = generate_local_wallet();
            let mut add_association_request = client
                .identity_updates()
                .associate_identity(new_wallet.identifier())
                .await
                .unwrap();

            add_wallet_signature(&mut add_association_request, &new_wallet).await;

            client
                .identity_updates()
                .apply_signature_request(add_association_request)
                .await
                .unwrap();
        }

        // Create a new client to test group operations with
        let other_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let other_conn = other_client.context.db();
        let ids = inbox_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
        // Load all the identity updates for the new inboxes
        load_identity_updates(other_client.context.api(), &other_conn, ids.as_slice())
            .await
            .expect("load should succeed");

        // Get the latest sequence IDs so we can construct the updates
        let latest_sequence_ids = other_conn.get_latest_sequence_id(ids.as_slice()).unwrap();

        let inbox_1_first_sequence_id = other_conn
            .get_identity_updates(inbox_ids[0].clone(), None, None)
            .unwrap()
            .first()
            .unwrap()
            .sequence_id;

        let mut original_group_membership = GroupMembership::new();
        original_group_membership.add(inbox_ids[0].to_string(), inbox_1_first_sequence_id as u64);
        original_group_membership.add(
            inbox_ids[1].to_string(),
            *latest_sequence_ids.get(&inbox_ids[1]).unwrap() as u64,
        );

        let mut new_group_membership = original_group_membership.clone();
        // Update the first inbox to have a higher sequence ID, but no new installations
        new_group_membership.add(
            inbox_ids[0].to_string(),
            *latest_sequence_ids.get(&inbox_ids[0]).unwrap() as u64,
        );
        new_group_membership.add(
            inbox_ids[2].to_string(),
            *latest_sequence_ids.get(&inbox_ids[2]).unwrap() as u64,
        );
        new_group_membership.remove(&inbox_ids[1]);

        let membership_diff = original_group_membership.diff(&new_group_membership);

        let installation_diff = other_client
            .identity_updates()
            .get_installation_diff(
                &other_conn,
                &GroupId::default(),
                &original_group_membership,
                &new_group_membership,
                &membership_diff,
            )
            .await
            .unwrap();

        assert_eq!(installation_diff.added_installations.len(), 1);
        assert!(
            installation_diff
                .added_installations
                .contains_key(&client_3_installation_key.to_vec()),
        );
        assert_eq!(installation_diff.removed_installations.len(), 1);
        assert!(
            installation_diff
                .removed_installations
                .contains(&client_2_installation_key.to_vec())
        );
    }

    /// The diff must reject an inbox that a commit adds at `sequence_id: 0`.
    /// It must not read the receiver's latest identity state. If it does,
    /// receivers with different identity history fork the group.
    #[rstest::rstest]
    #[xmtp_common::test]
    async fn get_installation_diff_rejects_added_inbox_at_sequence_zero() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let mut signature_request: SignatureRequest = client
            .identity_updates()
            .create_inbox(wallet.identifier(), None)
            .await
            .unwrap();
        let inbox_id = signature_request.inbox_id().to_string();
        add_wallet_signature(&mut signature_request, &wallet).await;
        client
            .identity_updates()
            .apply_signature_request(signature_request)
            .await
            .unwrap();

        // This client holds all the identity updates. A read of the latest
        // state would succeed. So the `0` causes the rejection below.
        let other_client = ClientBuilder::new_test_client(&generate_local_wallet()).await;
        let other_conn = other_client.context.db();
        load_identity_updates(
            other_client.context.api(),
            &other_conn,
            &[inbox_id.as_str()],
        )
        .await
        .expect("load should succeed");
        let latest_sequence_id = *other_conn
            .get_latest_sequence_id(&[inbox_id.as_str()])
            .unwrap()
            .get(&inbox_id)
            .unwrap();

        let old_membership = GroupMembership::new();

        // The same add at the real sequence id succeeds.
        let mut honest_membership = GroupMembership::new();
        honest_membership.add(inbox_id.clone(), latest_sequence_id as u64);
        other_client
            .identity_updates()
            .get_installation_diff(
                &other_conn,
                &GroupId::default(),
                &old_membership,
                &honest_membership,
                &old_membership.diff(&honest_membership),
            )
            .await
            .expect("an add at a real sequence id is valid");

        // Now the attack. Same inbox, same receiver, but sequence id 0.
        let mut crafted_membership = GroupMembership::new();
        crafted_membership.add(inbox_id.clone(), 0);
        let crafted_diff = old_membership.diff(&crafted_membership);
        assert_eq!(crafted_diff.added_inboxes, vec![&inbox_id]);

        let result = other_client
            .identity_updates()
            .get_installation_diff(
                &other_conn,
                &GroupId::default(),
                &old_membership,
                &crafted_membership,
                &crafted_diff,
            )
            .await;

        assert!(
            result.is_err(),
            "an inbox added at sequence_id 0 must be rejected, not resolved at latest"
        );
    }

    /// One identity update revokes at most its action limit, so a larger
    /// revocation fails before any request exists to sign, from either
    /// helper. A revocation at the limit is one request with that many actions.
    #[xmtp_common::test(unwrap_try = true)]
    async fn revoke_requests_over_action_limit_fail_before_signing() {
        use crate::client::ClientError;
        use xmtp_configuration::MAX_IDENTITY_UPDATE_ACTIONS;
        tester!(alix);
        let installations = |count| (0..count).map(|_| rand_vec::<32>()).collect::<Vec<_>>();
        let identities = |count| {
            (0..count)
                .map(|_| generate_local_wallet().identifier())
                .collect::<Vec<_>>()
        };
        let revoke_installations = |count| {
            super::revoke_installations_with_verifier(
                &alix.identifier(),
                alix.inbox_id(),
                installations(count),
            )
        };

        let over = MAX_IDENTITY_UPDATE_ACTIONS + 1;
        for error in [
            revoke_installations(over).unwrap_err(),
            alix.identity_updates()
                .revoke_identities(identities(over))
                .await
                .unwrap_err(),
        ] {
            assert!(
                matches!(
                    error,
                    ClientError::SignatureRequest(SignatureRequestError::TooManyActions(count))
                        if count == over
                ),
                "{error}"
            );
        }

        for mut request in [
            revoke_installations(MAX_IDENTITY_UPDATE_ACTIONS)?,
            alix.identity_updates()
                .revoke_identities(identities(MAX_IDENTITY_UPDATE_ACTIONS))
                .await?,
        ] {
            add_wallet_signature(&mut request, &alix.builder.owner).await;
            assert_eq!(
                request.build_identity_update()?.actions.len(),
                MAX_IDENTITY_UPDATE_ACTIONS
            );
        }
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    pub async fn revoke_wallet() {
        let recovery_wallet = generate_local_wallet();
        let second_wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&recovery_wallet).await;

        let mut add_wallet_signature_request = client
            .identity_updates()
            .associate_identity(second_wallet.identifier())
            .await
            .unwrap();

        add_wallet_signature(&mut add_wallet_signature_request, &second_wallet).await;

        client
            .identity_updates()
            .apply_signature_request(add_wallet_signature_request)
            .await
            .unwrap();

        let association_state_after_add = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(association_state_after_add.identifiers().len(), 2);

        // Make sure the inbox ID is correctly registered
        let inbox_ids = client
            .context
            .api()
            .get_inbox_ids(vec![second_wallet.identifier().into()])
            .await
            .unwrap();
        assert_eq!(inbox_ids.len(), 1);

        // Now revoke the second wallet

        let mut revoke_signature_request = client
            .identity_updates()
            .revoke_identities(vec![second_wallet.identifier()])
            .await
            .unwrap();
        add_wallet_signature(&mut revoke_signature_request, &recovery_wallet).await;

        client
            .identity_updates()
            .apply_signature_request(revoke_signature_request)
            .await
            .unwrap();

        // Make sure that the association state has removed the second wallet
        let association_state_after_revoke =
            get_association_state(&client, client.inbox_id()).await;
        assert_eq!(association_state_after_revoke.identifiers().len(), 1);

        // Make sure the inbox ID is correctly unregistered
        let inbox_ids = client
            .context
            .api()
            .get_inbox_ids(vec![second_wallet.identifier().into()])
            .await
            .unwrap();
        assert_eq!(inbox_ids, vec![None]);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    pub async fn revoke_installation() {
        use xmtp_events::{ClientEvent, EventFilter, EventKind};
        let wallet = generate_local_wallet();
        let client1: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let client2: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;

        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        // Ensure there are two installations on the inbox
        assert_eq!(association_state.installation_ids().len(), 2);
        let events = client1.context.events().subscribe(
            EventFilter::new([EventKind::IdentityOwnInstallationRevoked]),
            Some(10),
        );

        // Now revoke the second client
        let mut revoke_installation_request = client1
            .identity_updates()
            .revoke_installations(vec![client2.installation_public_key().to_vec()])
            .await
            .unwrap();
        add_wallet_signature(&mut revoke_installation_request, &wallet).await;
        assert!(events.drain().is_empty());
        client1
            .identity_updates()
            .apply_signature_request(revoke_installation_request)
            .await
            .unwrap();

        // Make sure there is only one installation on the inbox
        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        assert_eq!(association_state.installation_ids().len(), 1);
        let revoked = events.drain();
        assert_eq!(revoked.len(), 1);
        assert!(matches!(&revoked[0].client,
            Some(ClientEvent::IdentityOwnInstallationRevoked(event))
                if event.installation_key == client2.installation_public_key()
                   && !event.is_this_installation));
    }

    // verifies: EVENT-001
    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn direct_own_inbox_refresh_reports_each_installation_transition() {
        use xmtp_events::{ClientEvent, EventFilter, EventKind};
        let wallet = generate_local_wallet();
        let observer: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        observer.inbox_state(false).await?;
        let events = observer.context.events().subscribe(
            EventFilter::new([
                EventKind::IdentityOwnInstallationAdded,
                EventKind::IdentityOwnInstallationRevoked,
            ]),
            Some(10),
        );

        let transient: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let actor: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let mut request = actor
            .identity_updates()
            .revoke_installations(vec![transient.installation_public_key().to_vec()])
            .await?;
        add_wallet_signature(&mut request, &wallet).await;
        actor
            .identity_updates()
            .apply_signature_request(request)
            .await?;

        observer.inbox_state(true).await?;
        let changes = events.drain();
        assert_eq!(changes.len(), 3);
        assert!(changes.iter().any(|event| matches!(
            &event.client,
            Some(ClientEvent::IdentityOwnInstallationAdded(added))
                if added.installation_key == transient.installation_public_key().to_vec()
        )));
        assert!(changes.iter().any(|event| matches!(
            &event.client,
            Some(ClientEvent::IdentityOwnInstallationRevoked(revoked))
                if revoked.installation_key == transient.installation_public_key().to_vec()
        )));
        assert!(changes.iter().any(|event| matches!(
            &event.client,
            Some(ClientEvent::IdentityOwnInstallationAdded(added))
                if added.installation_key == actor.installation_public_key().to_vec()
        )));
    }

    // verifies: EVENT-001
    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn concurrent_own_inbox_refresh_emits_one_addition() {
        use xmtp_events::{ClientEvent, EventFilter, EventKind};

        let wallet = generate_local_wallet();
        let observer: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        observer.inbox_state(false).await?;
        let events = observer.context.events().subscribe(
            EventFilter::new([EventKind::IdentityOwnInstallationAdded]),
            Some(10),
        );
        let added: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;

        let (first, second) = tokio::join!(observer.inbox_state(true), observer.inbox_state(true));
        first?;
        second?;
        let additions = events.drain();
        assert!(matches!(additions.as_slice(), [event]
            if matches!(&event.client,
                Some(ClientEvent::IdentityOwnInstallationAdded(value))
                    if value.installation_key == added.installation_public_key().to_vec()
            )
        ));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test(flavor = "multi_thread")]
    pub async fn revoke_installation_with_malformed_keypackage() {
        use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

        let wallet = generate_local_wallet();
        let client1: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let client2: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;

        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        // Ensure there are two installations on the inbox
        assert_eq!(association_state.installation_ids().len(), 2);

        set_test_mode_upload_malformed_keypackage(
            true,
            Some(vec![client2.installation_public_key().to_vec()]),
        );

        // Now revoke the second client
        let mut revoke_installation_request = client1
            .identity_updates()
            .revoke_installations(vec![client2.installation_public_key().to_vec()])
            .await
            .unwrap();
        add_wallet_signature(&mut revoke_installation_request, &wallet).await;
        client1
            .identity_updates()
            .apply_signature_request(revoke_installation_request)
            .await
            .unwrap();

        // Make sure there is only one installation on the inbox
        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        assert_eq!(association_state.installation_ids().len(), 1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test(flavor = "multi_thread")]
    pub async fn revoke_good_installation_with_other_malformed_keypackage() {
        use crate::utils::test_mocks_helpers::set_test_mode_upload_malformed_keypackage;

        let wallet = generate_local_wallet();
        let client1: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let client2: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let client3: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;

        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        // Ensure there are two installations on the inbox
        assert_eq!(association_state.installation_ids().len(), 3);

        set_test_mode_upload_malformed_keypackage(
            true,
            Some(vec![client2.installation_public_key().to_vec()]),
        );

        // Now revoke the second client
        let mut revoke_installation_request = client1
            .identity_updates()
            .revoke_installations(vec![client3.installation_public_key().to_vec()])
            .await
            .unwrap();
        add_wallet_signature(&mut revoke_installation_request, &wallet).await;
        client1
            .identity_updates()
            .apply_signature_request(revoke_installation_request)
            .await
            .unwrap();

        // Make sure there is only one installation on the inbox
        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        assert_eq!(association_state.installation_ids().len(), 2);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    pub async fn change_recovery_address() {
        let original_wallet = generate_local_wallet();
        let new_recovery_wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&original_wallet).await;

        // Verify initial state has the original wallet as recovery identifier
        let association_state_before = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(
            association_state_before.recovery_identifier(),
            &original_wallet.identifier()
        );

        // Verify that the associated wallet at this stage includes the recovery address
        assert!(association_state_before.members().len() == 2);
        // Verify that one of the members is the recovery address
        let binding = association_state_before.members();
        let recovery_member = binding
            .iter()
            .find(|m| m.identifier == original_wallet.identifier());
        assert!(recovery_member.is_some());
        let recovery_member_timestamp = recovery_member.unwrap().client_timestamp_ns;
        // Right now we are not saving client side timestamps for recovery address, so this will be None
        assert!(recovery_member_timestamp.is_none());
        // Verify the other member is an installation key
        let installation_member = binding
            .iter()
            .find(|m| matches!(m.identifier, MemberIdentifier::Installation(_)));
        assert!(installation_member.is_some());
        assert!(
            installation_member
                .unwrap()
                .identifier
                .installation_key()
                .unwrap()
                == client.installation_public_key().to_vec()
        );
        let installation_member_timestamp = installation_member.unwrap().client_timestamp_ns;
        assert!(installation_member_timestamp.is_some());

        // Recovery moves only to a member, so associate the new recovery wallet first
        let mut associate_request = client
            .identity_updates()
            .associate_identity(new_recovery_wallet.identifier())
            .await
            .unwrap();
        add_wallet_signature(&mut associate_request, &new_recovery_wallet).await;
        client
            .identity_updates()
            .apply_signature_request(associate_request)
            .await
            .unwrap();

        // Create a signature request to change the recovery address
        let mut change_recovery_request = client
            .identity_updates()
            .change_recovery_identifier(new_recovery_wallet.identifier())
            .await
            .unwrap();

        // Add the original wallet's signature (since it's the current recovery address)
        add_wallet_signature(&mut change_recovery_request, &original_wallet).await;

        // Apply the signature request
        client
            .identity_updates()
            .apply_signature_request(change_recovery_request)
            .await
            .unwrap();

        // Verify the recovery address has been updated
        let association_state_after = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(
            association_state_after.recovery_identifier(),
            &new_recovery_wallet.identifier()
        );

        // Verify that the members are the original wallet, the new recovery wallet, and the
        // installation
        assert!(association_state_after.members().len() == 3);
        // Verify that one of the members is the recovery address
        let binding = association_state_after.members();
        let recovery_member = binding
            .iter()
            .find(|m| m.identifier == original_wallet.identifier());
        assert!(recovery_member.is_some());
        let recovery_member_timestamp = recovery_member.unwrap().client_timestamp_ns;
        // Right now we are not saving client side timestamps for recovery address, so this will be None
        assert!(recovery_member_timestamp.is_none());
        // Verify the other member is an installation key
        let installation_member = binding
            .iter()
            .find(|m| matches!(m.identifier, MemberIdentifier::Installation(_)));
        assert!(installation_member.is_some());
        assert!(
            installation_member
                .unwrap()
                .identifier
                .installation_key()
                .unwrap()
                == client.installation_public_key().to_vec()
        );
        let installation_member_timestamp = installation_member.unwrap().client_timestamp_ns;
        assert!(installation_member_timestamp.is_some());

        // Verify that the original wallet can no longer perform recovery operations
        // by attempting to revoke an installation with the original wallet
        let installation_id = client.installation_public_key().to_vec();
        let mut revoke_installation_request = client
            .identity_updates()
            .revoke_installations(vec![installation_id])
            .await
            .unwrap();

        // Try to sign with the original wallet (will error since signer is not in the request)
        // add_wallet_signature(&mut revoke_installation_request, &original_wallet).await;
        let signature_text = revoke_installation_request.signature_text();
        let sig = original_wallet
            .sign_message(signature_text.as_bytes())
            .await
            .unwrap()
            .as_bytes()
            .to_vec();
        let unverified_sig = UnverifiedSignature::new_recoverable_ecdsa(sig);
        let scw_verifier = MockSmartContractSignatureVerifier::new(false);

        let attempt_to_revoke_with_original_wallet = revoke_installation_request
            .add_signature(unverified_sig, &scw_verifier)
            .await;

        assert!(matches!(
            attempt_to_revoke_with_original_wallet,
            Err(SignatureRequestError::UnknownSigner)
        ));

        // Now try with the new recovery wallet (which should succeed)
        let installation_id = client.installation_public_key().to_vec();
        let mut revoke_installation_request = client
            .identity_updates()
            .revoke_installations(vec![installation_id])
            .await
            .unwrap();

        // Sign with the new recovery wallet
        add_wallet_signature(&mut revoke_installation_request, &new_recovery_wallet).await;

        // This should succeed because the new wallet is now the recovery address
        client
            .identity_updates()
            .apply_signature_request(revoke_installation_request)
            .await
            .unwrap();

        // Verify the installation was revoked
        let association_state_final = get_association_state(&client, client.inbox_id()).await;
        assert_eq!(association_state_final.installation_ids().len(), 0);
    }

    /// Inboxes [`store_scw_inboxes`] stores for each batch read.
    const INBOXES: u64 = 20;

    /// Records the most verify batches in flight at once. The batch for the
    /// inbox created at block `n` yields `INBOXES - n` times, so later inboxes
    /// finish first.
    #[derive(Default)]
    struct BatchesInFlight {
        now: AtomicUsize,
        most: AtomicUsize,
    }

    #[xmtp_common::async_trait]
    impl SmartContractSignatureVerifier for BatchesInFlight {
        async fn is_valid_signature(
            &self,
            account_id: AccountId,
            hash: [u8; 32],
            signature: Bytes,
            block_number: Option<BlockNumber>,
        ) -> Result<ValidationResponse, VerifierError> {
            let input = ScwInput::checked(account_id, hash, signature, block_number)?;
            Ok(self.verify_many(&[input]).await?.remove(0))
        }

        async fn verify_many(
            &self,
            inputs: &[ScwInput],
        ) -> Result<Vec<ValidationResponse>, VerifierError> {
            let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.most.fetch_max(now, Ordering::SeqCst);
            for _ in inputs[0].block_number().unwrap_or_default()..INBOXES {
                xmtp_common::task::yield_now().await;
            }
            self.now.fetch_sub(1, Ordering::SeqCst);
            Ok(inputs
                .iter()
                .map(|_| ValidationResponse {
                    is_valid: true,
                    block_number: Some(1),
                    error: None,
                })
                .collect())
        }
    }

    /// Store [`INBOXES`] inboxes, the one at index `n` created by a smart
    /// contract wallet at block `n`, and return their identifiers in order.
    fn store_scw_inboxes<C: ConnectionExt>(conn: &DbConnection<C>) -> Vec<(String, Option<i64>)> {
        (0..INBOXES)
            .map(|block| {
                let wallet = Identifier::rand_ethereum();
                let inbox_id = wallet.inbox_id(0).unwrap();
                let create = UnverifiedAction::CreateInbox(UnverifiedCreateInbox::new(
                    UnsignedCreateInbox {
                        nonce: 0,
                        account_identifier: wallet.clone(),
                    },
                    UnverifiedSignature::new_smart_contract_wallet(
                        vec![1],
                        AccountId::new_evm(1, wallet.to_string()),
                        block,
                    ),
                ));
                let update = IdentityUpdateProto::from(UnverifiedIdentityUpdate::new(
                    inbox_id.clone(),
                    1,
                    vec![create],
                ));
                conn.insert_or_ignore_identity_updates(&[StoredIdentityUpdate::new(
                    inbox_id.clone(),
                    1,
                    0,
                    update.encode_to_vec(),
                )])
                .unwrap();
                (inbox_id, None)
            })
            .collect()
    }

    /// A batch read of many inboxes derives at most eight states at once, so
    /// a group's members never open one verify batch each at the same time.
    /// The states come back in the order of the identifiers, although later
    /// ones finish first.
    #[xmtp_common::test(unwrap_try = true)]
    async fn batch_association_state_reads_keep_eight_in_flight() {
        let verifier = Arc::new(BatchesInFlight::default());
        let client = ClientBuilder::new_test_builder(&generate_local_wallet())
            .await
            .with_scw_verifier(verifier.clone())
            .build()
            .await?;
        let conn = client.context.db();
        let inbox_ids = |identifiers: &[(String, Option<i64>)]| {
            identifiers
                .iter()
                .map(|(inbox_id, _)| inbox_id.clone())
                .collect::<Vec<_>>()
        };
        let state_ids = |states: Vec<AssociationState>| {
            states
                .iter()
                .map(|state| state.inbox_id().to_string())
                .collect::<Vec<_>>()
        };

        let identifiers = store_scw_inboxes(&conn);
        let states =
            batch_get_association_state_with_verifier(&conn, &identifiers, &verifier).await?;
        assert_eq!(verifier.most.swap(0, Ordering::SeqCst), 8);
        assert_eq!(state_ids(states), inbox_ids(&identifiers));

        let identifiers = store_scw_inboxes(&conn);
        let states = IdentityUpdates::new(&client.context)
            .batch_get_association_state(&conn, &identifiers)
            .await?;
        assert_eq!(verifier.most.load(Ordering::SeqCst), 8);
        assert_eq!(state_ids(states), inbox_ids(&identifiers));
    }
}

#[cfg(test)]
mod conflict_tests {
    use super::*;
    use crate::tester;
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
        tester!(alix, disable_workers);
        let wallet = generate_local_wallet();
        let mut request = alix
            .identity_updates()
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

    #[xmtp_common::test(flavor = "multi_thread", worker_threads = 4, unwrap_try = true)]
    async fn two_clients_racing_identity_updates_keep_both_associations() {
        tester!(alix, disable_workers);
        tester!(alix2, from: alix, disable_workers);
        use xmtp_proto::api::HasStats;
        let first_stats = alix.context.api().api_client.mls_stats();
        let second_stats = alix2.context.api().api_client.mls_stats();
        const MAX_RACE_ROUNDS: usize = 16;
        const UPDATES_PER_RACE: usize = 2;
        for _ in 0..MAX_RACE_ROUNDS {
            let first_wallet = generate_local_wallet();
            let second_wallet = generate_local_wallet();
            let mut first = alix
                .identity_updates()
                .associate_identity(first_wallet.identifier())
                .await?;
            let mut second = alix2
                .identity_updates()
                .associate_identity(second_wallet.identifier())
                .await?;
            add_wallet_signature(&mut first, &first_wallet).await;
            add_wallet_signature(&mut second, &second_wallet).await;
            let first_updates = alix.identity_updates();
            let second_updates = alix2.identity_updates();
            first_stats.clear();
            second_stats.clear();
            let (first, second) = {
                xmtp_common::wasm_or_native! {
                    native => {
                        // Start both RPCs on separate threads. Require a retry below.
                        let barrier = std::sync::Barrier::new(UPDATES_PER_RACE);
                        let runtime = tokio::runtime::Handle::current();
                        std::thread::scope(|scope| {
                            let first = scope.spawn(|| {
                                barrier.wait();
                                runtime.block_on(first_updates.apply_signature_request(first))
                            });
                            let second = scope.spawn(|| {
                                barrier.wait();
                                runtime.block_on(second_updates.apply_signature_request(second))
                            });
                            (first.join().unwrap(), second.join().unwrap())
                        })
                    },
                    wasm => {
                        futures::join!(
                            first_updates.apply_signature_request(first),
                            second_updates.apply_signature_request(second),
                        )
                    }
                }
            };
            first?;
            second?;
            let publishes = first_stats.publish.get_count() + second_stats.publish.get_count();
            let conn = alix.context.db();
            load_identity_updates(alix.context.api(), &conn, &[alix.inbox_id()]).await?;
            let state = alix
                .identity_updates()
                .get_association_state(&conn, alix.inbox_id(), None)
                .await?;
            assert!(state.get(&first_wallet.identifier().into()).is_some());
            assert!(state.get(&second_wallet.identifier().into()).is_some());
            if publishes > UPDATES_PER_RACE {
                tracing::info!(publishes, "Both identity updates survived a publish retry");
                return;
            }
        }
        panic!("No identity publish retry occurred in {MAX_RACE_ROUNDS} races");
    }
}
