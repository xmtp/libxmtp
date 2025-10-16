use crate::{
    XmtpApi,
    client::ClientError,
    context::XmtpSharedContext,
    groups::group_membership::{GroupMembership, MembershipDiff},
    identity::IdentityError,
    subscriptions::SyncWorkerEvent,
};
use futures::future::try_join_all;
use std::collections::{HashMap, HashSet};
use thiserror::Error;
use xmtp_common::{Retry, RetryableError, retry_async, retryable};
use xmtp_configuration::Originators;
use xmtp_cryptography::CredentialSign;
use xmtp_db::StorageError;
use xmtp_db::XmtpDb;
use xmtp_db::prelude::*;
use xmtp_db::{db_connection::DbConnection, identity_update::StoredIdentityUpdate};
use xmtp_id::{
    AsIdRef, InboxIdRef,
    associations::{
        AssociationError, AssociationState, AssociationStateDiff, Identifier, IdentityAction,
        IdentityUpdate, InstallationKeyContext, MemberIdentifier, SignatureError, apply_update,
        builder::{SignatureRequest, SignatureRequestBuilder, SignatureRequestError},
        get_state,
        unverified::{
            UnverifiedIdentityUpdate, UnverifiedInstallationKeySignature, UnverifiedSignature,
        },
    },
    scw_verifier::{RemoteSignatureVerifier, SmartContractSignatureVerifier},
};
use xmtp_proto::api_client::{XmtpIdentityClient, XmtpMlsClient};

use xmtp_api::{ApiClientWrapper, GetIdentityUpdatesV2Filter};
use xmtp_id::InboxUpdate;

#[derive(Debug, Error)]
pub enum IdentityUpdateError {
    #[error(transparent)]
    InvalidSignatureRequest(#[from] SignatureRequestError),
}

#[derive(Debug)]
pub struct InstallationDiff {
    pub added_installations: HashSet<Vec<u8>>,
    pub removed_installations: HashSet<Vec<u8>>,
}

#[derive(Debug, Error)]
pub enum InstallationDiffError {
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
            InstallationDiffError::Client(client_error) => retryable!(client_error),
            InstallationDiffError::Storage(e) => retryable!(e),
            InstallationDiffError::Db(e) => retryable!(e),
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
pub async fn get_association_state_with_verifier(
    conn: &impl DbQuery,
    inbox_id: &str,
    to_sequence_id: Option<i64>,
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<AssociationState, ClientError> {
    let updates = conn.get_identity_updates(inbox_id, None, to_sequence_id)?;
    let last_sequence_id = updates
        .last()
        .ok_or::<ClientError>(AssociationError::MissingIdentityUpdate.into())?
        .sequence_id;
    if let Some(to_sequence_id) = to_sequence_id
        && to_sequence_id != last_sequence_id
    {
        return Err(AssociationError::MissingIdentityUpdate.into());
    }

    if let Some(association_state) = conn.read_from_cache(inbox_id, last_sequence_id)? {
        return Ok(association_state.try_into().map_err(StorageError::from)?);
    }

    let unverified_updates = updates
        .into_iter()
        // deserialize identity update payload
        .map(UnverifiedIdentityUpdate::try_from)
        .collect::<Result<Vec<UnverifiedIdentityUpdate>, AssociationError>>()?;
    let updates = verify_updates(unverified_updates, scw_verifier).await?;

    let association_state = get_state(updates)?;

    conn.write_to_cache(
        inbox_id.to_owned(),
        last_sequence_id,
        association_state.clone().into(),
    )?;

    Ok(association_state)
}

/// Revoke the given installations from the association state for the client's inbox
pub fn revoke_installations_with_verifier(
    identifier: &Identifier,
    inbox_id: &str,
    installation_ids: Vec<Vec<u8>>,
) -> Result<SignatureRequest, ClientError> {
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
    signature_request: SignatureRequest,
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<(), ClientError> {
    // If the signature request isn't completed, this will error
    let identity_update = signature_request
        .build_identity_update()
        .map_err(IdentityUpdateError::from)?;

    identity_update.to_verified(scw_verifier).await?;

    // We don't need to validate the update, since the server will do this for us
    api_client.publish_identity_update(identity_update).await?;

    Ok(())
}

/// Get the association state for all provided `inbox_id`/optional `sequence_id` tuples, using the cache when available
/// If the association state is not available in the cache, this falls back to reconstructing the association state
/// from Identity Updates in the network.
pub async fn batch_get_association_state_with_verifier(
    conn: &impl DbQuery,
    identifiers: &[(impl AsIdRef, Option<i64>)],
    scw_verifier: &impl SmartContractSignatureVerifier,
) -> Result<Vec<AssociationState>, ClientError> {
    let association_states = try_join_all(
        identifiers
            .iter()
            .map(|(inbox_id, to_sequence_id)| {
                get_association_state_with_verifier(
                    conn,
                    inbox_id.as_ref(),
                    *to_sequence_id,
                    scw_verifier,
                )
            })
            .collect::<Vec<_>>(),
    )
    .await?;

    Ok(association_states)
}

impl<'a, Context> IdentityUpdates<Context>
where
    Context: XmtpSharedContext,
{
    /// Get the association state for all provided `inbox_id`/optional `sequence_id` tuples, using the cache when available
    /// If the association state is not available in the cache, this falls back to reconstructing the association state
    /// from Identity Updates in the network.
    pub async fn batch_get_association_state(
        &self,
        conn: &impl DbQuery,
        identifiers: &[(impl AsIdRef, Option<i64>)],
    ) -> Result<Vec<AssociationState>, ClientError> {
        batch_get_association_state_with_verifier(conn, identifiers, &self.context.scw_verifier())
            .await
    }

    /// Get the latest association state available on the network for the given `inbox_id`
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_latest_association_state(
        &self,
        conn: &DbConnection<<Context::Db as XmtpDb>::Connection>,
        inbox_id: InboxIdRef<'a>,
    ) -> Result<AssociationState, ClientError> {
        load_identity_updates(self.context.api(), conn, &[inbox_id]).await?;

        self.get_association_state(conn, inbox_id, None).await
    }

    /// Get the association state for a given inbox_id up to the (and inclusive of) the `to_sequence_id`
    /// If no `to_sequence_id` is provided, use the latest value in the database
    pub async fn get_association_state(
        &self,
        conn: &impl xmtp_db::DbQuery,
        inbox_id: InboxIdRef<'a>,
        to_sequence_id: Option<i64>,
    ) -> Result<AssociationState, ClientError> {
        get_association_state_with_verifier(
            conn,
            inbox_id,
            to_sequence_id,
            &self.context.scw_verifier(),
        )
        .await
    }

    /// Calculate the changes between the `starting_sequence_id` and `ending_sequence_id` for the
    /// provided `inbox_id`
    pub(crate) async fn get_association_state_diff(
        &self,
        conn: &impl DbQuery,
        inbox_id: InboxIdRef<'a>,
        starting_sequence_id: Option<i64>,
        ending_sequence_id: Option<i64>,
    ) -> Result<AssociationStateDiff, ClientError> {
        tracing::debug!(
            "Computing diff for {:?} from {:?} to {:?}",
            inbox_id,
            starting_sequence_id,
            ending_sequence_id
        );
        // If no starting sequence ID, get all updates from the beginning of the inbox's history up to the ending sequence ID
        if starting_sequence_id.is_none() {
            return Ok(self
                .get_association_state(conn, inbox_id, ending_sequence_id)
                .await?
                .as_diff());
        }

        // Get the initial state to compare against
        let initial_state = self
            .get_association_state(conn, inbox_id, starting_sequence_id)
            .await?;

        // Get any identity updates that need to be applied
        let incremental_updates =
            conn.get_identity_updates(inbox_id, starting_sequence_id, ending_sequence_id)?;

        let last_sequence_id = incremental_updates.last().map(|update| update.sequence_id);
        if ending_sequence_id.is_some()
            && last_sequence_id.is_some()
            && last_sequence_id != ending_sequence_id
        {
            tracing::error!(
                "Did not find the expected last sequence id. Expected: {:?}, Found: {:?}",
                ending_sequence_id,
                last_sequence_id
            );
            return Err(AssociationError::MissingIdentityUpdate.into());
        }

        let unverified_incremental_updates: Vec<UnverifiedIdentityUpdate> = incremental_updates
            .into_iter()
            .map(|update| update.try_into())
            .collect::<Result<Vec<UnverifiedIdentityUpdate>, AssociationError>>()?;

        let incremental_updates =
            verify_updates(unverified_incremental_updates, &self.context.scw_verifier()).await?;
        let mut final_state = initial_state.clone();
        // Apply each update sequentially, aborting in the case of error
        for update in incremental_updates {
            final_state = apply_update(final_state, update)?;
        }

        tracing::debug!("Final state at {:?}: {:?}", last_sequence_id, final_state);
        if let Some(last_sequence_id) = last_sequence_id {
            conn.write_to_cache(
                inbox_id.to_string(),
                last_sequence_id,
                final_state.clone().into(),
            )?;
        }

        Ok(initial_state.diff(&final_state))
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

        Ok(signature_request)
    }

    /// Revoke the given identities from the association state for the client's inbox
    pub async fn revoke_identities(
        &self,
        identities_to_revoke: Vec<Identifier>,
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

        for ident in identities_to_revoke {
            builder = builder.revoke_association(
                current_state.recovery_identifier().clone().into(),
                ident.into(),
            )
        }

        Ok(builder.build())
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

        let result = revoke_installations_with_verifier(
            &current_state.recovery_identifier().clone(),
            inbox_id,
            installation_ids,
        )?;

        let _ = self
            .context
            .worker_events()
            .send(SyncWorkerEvent::CycleHMAC);

        Ok(result)
    }

    /// Generate a `ChangeRecoveryAddress` signature request using a new identifer
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
        Ok(builder.build())
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
        let inbox_id = signature_request.inbox_id().to_string();

        apply_signature_request_with_verifier(
            self.context.api(),
            signature_request,
            &self.context.scw_verifier(),
        )
        .await?;

        // Load the identity updates for the inbox so that we have a record in our DB
        retry_async!(
            Retry::default(),
            (async {
                load_identity_updates(self.context.api(), &self.context.db(), &[inbox_id.as_str()])
                    .await
            })
        )?;

        Ok(())
    }

    /// Given two group memberships and the diff, get the list of installations that were added or removed
    /// between the two membership states.
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn get_installation_diff(
        &self,
        conn: &impl DbQuery,
        old_group_membership: &GroupMembership,
        new_group_membership: &GroupMembership,
        membership_diff: &MembershipDiff<'_>,
    ) -> Result<InstallationDiff, InstallationDiffError> {
        tracing::info!(
            "Getting installation diff. Old: {:?}. New {:?}",
            old_group_membership,
            new_group_membership
        );
        let added_and_updated_members = membership_diff
            .added_inboxes
            .iter()
            .chain(membership_diff.updated_inboxes.iter());

        let filters = added_and_updated_members
            .clone()
            .map(|i| {
                (
                    i.as_str(),
                    new_group_membership.get(i).map(|i| *i as i64).unwrap_or(0),
                )
            })
            .collect::<Vec<(&str, i64)>>();

        load_identity_updates(
            self.context.api(),
            conn,
            &crate::groups::filter_inbox_ids_needing_updates(conn, filters.as_slice())?,
        )
        .await?;

        let mut added_installations: HashSet<Vec<u8>> = HashSet::new();
        let mut removed_installations: HashSet<Vec<u8>> = HashSet::new();

        // TODO: Do all of this in parallel
        for inbox_id in added_and_updated_members {
            let starting_sequence_id = match old_group_membership.get(inbox_id) {
                Some(0) => None,
                Some(i) => Some(*i as i64),
                None => None,
            };
            let state_diff = self
                .get_association_state_diff(
                    conn,
                    inbox_id.as_str(),
                    starting_sequence_id,
                    new_group_membership.get(inbox_id).map(|i| *i as i64),
                )
                .await?;

            added_installations.extend(state_diff.new_installations());
            removed_installations.extend(state_diff.removed_installations());
        }

        for inbox_id in membership_diff.removed_inboxes.iter() {
            let state_diff = self
                .get_association_state(
                    conn,
                    inbox_id,
                    old_group_membership.get(inbox_id).map(|i| *i as i64),
                )
                .await?
                .as_diff();

            // In the case of a removed member, get all the "new installations" from the diff and add them to the list of removed installations
            removed_installations.extend(state_diff.new_installations());
        }

        Ok(InstallationDiff {
            added_installations,
            removed_installations,
        })
    }
}

/// For the given list of `inbox_id`s get all updates from the network that are newer than the last known `sequence_id`,
/// write them in the db, and return the updates
#[tracing::instrument(level = "trace", skip_all)]
pub async fn load_identity_updates<ApiClient: XmtpApi>(
    api_client: &ApiClientWrapper<ApiClient>,
    conn: &impl xmtp_db::DbQuery,
    inbox_ids: &[&str],
) -> Result<HashMap<String, Vec<InboxUpdate>>, ClientError> {
    if inbox_ids.is_empty() {
        return Ok(HashMap::new());
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

    let updates = api_client
        .get_identity_updates_v2(filters)
        .await?
        .collect::<HashMap<_, Vec<InboxUpdate>>>();
    let to_store = updates
        .iter()
        .flat_map(move |(inbox_id, updates)| {
            updates.iter().map(move |update| StoredIdentityUpdate {
                inbox_id: inbox_id.clone(),
                sequence_id: update.sequence_id as i64,
                server_timestamp_ns: update.server_timestamp_ns as i64,
                payload: update.update.clone().into(),
                originator_id: Originators::INBOX_LOG.into(),
            })
        })
        .collect::<Vec<StoredIdentityUpdate>>();

    conn.insert_or_ignore_identity_updates(&to_store)?;
    Ok(updates)
}

/// Convert a list of unverified updates to verified updates using the given smart contract verifier
async fn verify_updates(
    updates: Vec<UnverifiedIdentityUpdate>,
    scw_verifier: impl SmartContractSignatureVerifier,
) -> Result<Vec<IdentityUpdate>, SignatureError> {
    try_join_all(
        updates
            .iter()
            .map(|update| update.to_verified(&scw_verifier)),
    )
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
    Client: XmtpMlsClient + XmtpIdentityClient + Clone + Send + Sync,
{
    let filters = vec![GetIdentityUpdatesV2Filter {
        inbox_id: inbox_id.to_string(),
        sequence_id: None,
    }];
    let mut updates = api_client
        .get_identity_updates_v2(filters)
        .await?
        .collect::<HashMap<xmtp_id::InboxId, Vec<InboxUpdate>>>();

    let Some(updates) = updates.remove(inbox_id) else {
        return Err(ClientError::Generic(
            "Unable to find provided inbox_id".to_string(),
        ));
    };
    let updates: Vec<_> = updates.into_iter().map(|u| u.update).collect();

    let mut association_state = None;

    let scw_verifier = scw_verifier.unwrap_or_else(|| {
        Box::new(RemoteSignatureVerifier::new(api_client.clone()))
            as Box<dyn SmartContractSignatureVerifier>
    });

    let updates: Vec<_> = updates
        .iter()
        .map(|u| u.to_verified(&scw_verifier))
        .collect();
    let updates = try_join_all(updates).await?;

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

    let unverified_update: UnverifiedIdentityUpdate = first_update.clone().try_into()?;

    let verified = unverified_update.to_verified(scw_verifier).await?;

    Ok(verified.creation_signature_kind())
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(unused)] // b/c wasm & native
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use crate::{
        Client, XmtpApi,
        builder::ClientBuilder,
        context::XmtpSharedContext,
        groups::group_membership::GroupMembership,
        identity_updates::IdentityUpdates,
        tester,
        utils::{FullXmtpClient, Tester},
    };
    use alloy::signers::Signer;
    use xmtp_api::IdentityUpdate;
    use xmtp_configuration::Originators;
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

    use xmtp_common::rand_vec;

    use super::{is_member_of_association_state, load_identity_updates};

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
        let identity_update = StoredIdentityUpdate::new(
            inbox_id.to_string(),
            sequence_id,
            0,
            rand_vec::<24>(),
            Originators::INBOX_LOG.into(),
        );

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
        let state = client_identity_updates
            .get_latest_association_state(&conn, client.inbox_id())
            .await
            .unwrap();

        // The installation, wallet1 address, and the newly associated wallet2 address
        assert_eq!(state.members().len(), 3);

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

        use xmtp_common::assert_logged;

        use crate::{groups::device_sync::DeviceSyncClient, worker::metrics::WorkerMetrics};

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

            get_association_state(&client, inbox_id).await;

            assert_logged!("Loaded association", 1);
            assert_logged!("Wrote association", 2);

            let association_state = get_association_state(&client, inbox_id).await;

            assert_logged!("Loaded association", 2);
            assert_logged!("Wrote association", 2);

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
        for (client, wallet) in vec![
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
                .contains(&client_3_installation_key.to_vec()),
        );
        assert_eq!(installation_diff.removed_installations.len(), 1);
        assert!(
            installation_diff
                .removed_installations
                .contains(&client_2_installation_key.to_vec())
        );
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
        assert_eq!(inbox_ids.len(), 0);
    }

    #[rstest::rstest]
    #[xmtp_common::test]
    pub async fn revoke_installation() {
        let wallet = generate_local_wallet();
        let client1: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;
        let client2: FullXmtpClient = ClientBuilder::new_test_client(&wallet).await;

        let association_state = get_association_state(&client1, client1.inbox_id()).await;
        // Ensure there are two installations on the inbox
        assert_eq!(association_state.installation_ids().len(), 2);

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

        // Verify that the associated wallet still includes the original wallet
        assert!(association_state_after.members().len() == 2);
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
}
