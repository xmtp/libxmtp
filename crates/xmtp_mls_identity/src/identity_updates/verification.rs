//! Association state reads and signature requests checked with an explicit
//! verifier, identifier membership, and creation signature kinds.

use super::*;

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
) -> Result<AssociationState, IdentityUpdateError> {
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
) -> Result<Option<bool>, IdentityUpdateError> {
    match get_association_state_with_verifier(conn, inbox_id, None, scw_verifier).await {
        Ok(state) => Ok(Some(state_holds_identifier(&state, identifier))),
        Err(IdentityUpdateError::Association(AssociationError::MissingIdentityUpdate)) => Ok(None),
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
) -> Result<Option<bool>, IdentityUpdateError> {
    match identifier_membership(conn, inbox_id, identifier, &NoRequestVerifier).await {
        Err(IdentityUpdateError::SignatureValidation(SignatureError::VerifierError(_))) => {
            Ok(Some(false))
        }
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
pub(super) fn check_revocation_count(count: usize) -> Result<(), IdentityUpdateError> {
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
) -> Result<SignatureRequest, IdentityUpdateError> {
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
) -> Result<(), IdentityUpdateError> {
    // If the signature request isn't completed, this will error
    let identity_update = signature_request
        .build_identity_update()
        .map_err(IdentityUpdateError::from)?;

    identity_update.to_verified(scw_verifier).await?;

    publish_with_conflict_retry(api_client, conn, identity_update, scw_verifier)
        .await
        .map_err(|error| IdentityUpdateError::Publish(Box::new(error)))?;

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
) -> Result<Vec<AssociationState>, IdentityUpdateError> {
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

/// A static lookup method to verify if an identity is a member of an inbox
pub async fn is_member_of_association_state<Client>(
    api_client: &ApiClientWrapper<Client>,
    inbox_id: &str,
    identifier: &MemberIdentifier,
    scw_verifier: Option<Box<dyn SmartContractSignatureVerifier>>,
) -> Result<bool, IdentityUpdateError>
where
    Client: XmtpBackendClient + Clone,
{
    let filters = vec![GetIdentityUpdatesV2Filter {
        inbox_id: inbox_id.to_string(),
        sequence_id: None,
    }];
    let mut updates = api_client.get_identity_updates_v2(filters).await?;

    let Some(updates) = updates.remove(inbox_id) else {
        return Err(IdentityUpdateError::InboxNotFound);
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
    let association_state = association_state.ok_or(IdentityUpdateError::EmptyIdentityLog)?;

    Ok(association_state.get(identifier).is_some())
}

#[tracing::instrument(level = "trace", skip_all)]
pub async fn get_creation_signature_kind(
    conn: &impl xmtp_db::DbQuery,
    scw_verifier: impl SmartContractSignatureVerifier,
    inbox_id: InboxIdRef<'_>,
) -> Result<Option<xmtp_id::associations::SignatureKind>, IdentityUpdateError> {
    let updates = conn.get_identity_updates(inbox_id, None, None)?;

    let first_update = updates
        .first()
        .ok_or_else(|| IdentityUpdateError::Identity(IdentityError::RequiredIdentityNotFound))?;

    let unverified_update: UnverifiedIdentityUpdate = first_update.clone().to_unverified()?;

    let verified = unverified_update.to_verified(scw_verifier).await?;

    Ok(verified.creation_signature_kind())
}
