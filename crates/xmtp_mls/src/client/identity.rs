//! Client identity reads: inbox lookups, states and update counts, credential
//! validation, reachability and handles.

use super::*;

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    /// The inbox that holds `identifier`, or `None`. The backend's answer
    /// counts only when the newest state the client derives for the answered
    /// inbox holds the identifier.
    pub async fn find_inbox_id_from_identifier(
        &self,
        identifier: Identifier,
    ) -> Result<Option<String>, ClientError> {
        Ok(
            resolve_verified_in(&self.context, &[IdentityTarget::Account(identifier)])
                .await?
                .pop()
                .flatten(),
        )
    }

    /// Get the highest `sequence_id` from the local database for the client's `inbox_id`.
    /// This may not be consistent with the latest state on the backend.
    pub fn inbox_sequence_id(
        &self,
        conn: &DbConnection<<Context::Db as XmtpDb>::Connection>,
    ) -> Result<i64, StorageError> {
        self.context
            .identity()
            .sequence_id(conn)
            .map_err(Into::into)
    }

    /// Get the [`AssociationState`] for the client's `inbox_id`
    pub async fn inbox_state(
        &self,
        refresh_from_network: bool,
    ) -> Result<AssociationState, ClientError> {
        let conn = self.context.db();
        let inbox_id = self.inbox_id();
        if refresh_from_network {
            load_identity_updates_for_client(&self.context, &conn, &[inbox_id]).await?;
        }
        let identity_service = IdentityUpdates::new(&self.context);
        let state = identity_service
            .get_association_state(&conn, inbox_id, None)
            .await?;
        Ok(state)
    }

    /// Get the [`AssociationState`] for each `inbox_id`
    pub async fn inbox_addresses(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<InboxIdRef<'_>>,
    ) -> Result<Vec<AssociationState>, ClientError> {
        let conn = self.context.db();
        if refresh_from_network {
            load_identity_updates_for_client(&self.context, &conn, &inbox_ids).await?;
        }
        let identity_service = IdentityUpdates::new(&self.context);
        let state = identity_service
            .batch_get_association_state(
                &conn,
                &inbox_ids.into_iter().map(|i| (i, None)).collect::<Vec<_>>(),
            )
            .await?;
        Ok(state)
    }

    /// Get the total number of inbox updates for `inbox_ids`. `refresh_from_network` will force
    /// a network refresh. May still access network if an inbox_id does not yet exist in the local
    /// cache.
    pub async fn fetch_inbox_updates_count(
        &self,
        refresh_from_network: bool,
        inbox_ids: Vec<InboxIdRef<'_>>,
    ) -> Result<HashMap<InboxId, u32>, ClientError> {
        let conn = self.context.db();
        if refresh_from_network {
            load_identity_updates_for_client(&self.context, &conn, &inbox_ids).await?;
        }
        let inbox_id_strs = inbox_ids.to_vec();
        let counts = conn.count_inbox_updates(&inbox_id_strs)?;
        Ok(counts.into_iter().map(|(k, v)| (k, v as u32)).collect())
    }

    /// Get the total number of inbox updates for the client's inbox_id.
    /// Setting `refresh_from_network` forces a network refresh, otherwise
    /// this operation is offline.
    pub async fn fetch_own_inbox_updates_count(
        &self,
        refresh_from_network: bool,
    ) -> Result<u32, ClientError> {
        let inbox_id = self.inbox_id();
        Ok(self
            .fetch_inbox_updates_count(refresh_from_network, vec![inbox_id])
            .await?
            .get(inbox_id)
            .copied()
            .unwrap_or(0))
    }

    /// Get the signature kind used to create an inbox.
    ///
    /// # Arguments
    /// * `inbox_id` - The inbox ID to check
    /// * `refresh_from_network` - Whether to fetch updates from the network first
    ///
    /// # Returns
    /// * `Some(SignatureKind)` - The signature kind used to create the inbox
    /// * `None` - Inbox doesn't exist or creation info is unavailable
    pub async fn inbox_creation_signature_kind(
        &self,
        inbox_id: InboxIdRef<'_>,
        refresh_from_network: bool,
    ) -> Result<Option<xmtp_id::associations::SignatureKind>, ClientError> {
        let conn = self.context.db();

        // Load the first identity update (creation update) for this inbox if requested
        if refresh_from_network {
            load_identity_updates_for_client(&self.context, &conn, &[inbox_id]).await?;
        }

        let verifier = self.context.scw_verifier();

        let signature_kind = get_creation_signature_kind(&conn, verifier, inbox_id).await?;

        Ok(signature_kind)
    }

    /**
     * Validates a credential against the given installation public key
     *
     * This will go to the network and get the latest association state for the inbox.
     * It ensures that the installation_pub_key is in that association state
     */
    pub async fn validate_credential_against_network(
        &self,
        conn: &DbConnection<<Context::Db as XmtpDb>::Connection>,
        credential: &[u8],
        installation_pub_key: Vec<u8>,
    ) -> Result<InboxId, ClientError> {
        let inbox_id = parse_credential(credential)?;
        let association_state = IdentityUpdates::new(&self.context)
            .get_latest_association_state(conn, &inbox_id)
            .await?;
        let ident = MemberIdentifier::installation(installation_pub_key);

        match association_state.get(&ident) {
            Some(_) => Ok(inbox_id),
            None => Err(IdentityError::InstallationIdNotFound(inbox_id).into()),
        }
    }

    /// Reports whether the backend names an inbox for each identifier.
    /// This is an unverified availability hint. The client does not independently
    /// verify the lookup against current association state or check
    /// key packages. This is not a delivery guarantee.
    ///
    /// Arguments:
    /// - account_identifier: a list of account identifiers to check
    ///
    /// Returns:
    /// A map from each identifier to its availability hint.
    pub async fn can_message(
        &self,
        account_identifiers: &[Identifier],
    ) -> Result<HashMap<Identifier, bool>, ClientError> {
        // A client with a blocked connection issues no request. The blocked connection
        // alone, not `ensure_identity_ready`, because this answers before the
        // caller has registered an identity.
        self.context.server_configuration().check()?;
        let requests = account_identifiers.iter().map(Into::into).collect();

        let results = self.context.api().get_inbox_ids(requests).await?;
        Ok(account_identifiers
            .iter()
            .cloned()
            .zip(results.into_iter().map(|inbox_id| inbox_id.is_some()))
            .collect())
    }
}

impl<Context: XmtpSharedContext> Client<Context> {
    /// The inbox that holds each handle, in order, or `None`. An answer from
    /// the backend counts only when the newest state the client derives for
    /// the answered inbox holds the handle. An old handle of an inbox still
    /// resolves to it.
    pub async fn resolve_handles(
        &self,
        handles: &[Handle],
    ) -> Result<Vec<Option<InboxId>>, ClientError> {
        Ok(inbox_handles::resolve_verified_in(
            &self.context,
            &inbox_handles::handle_targets(handles),
        )
        .await?)
    }

    /// Whether the backend names an inbox for each handle, in order. This
    /// checks no association state: use it only where a wrong answer costs a
    /// candidate, never to reach an inbox.
    pub async fn handles_have_owner(&self, handles: &[Handle]) -> Result<Vec<bool>, ClientError> {
        Ok(inbox_handles::handles_have_owner(&self.context, handles).await?)
    }

    /// The handles for `input` that no inbox holds. `input` is a handle as a
    /// user typed it; its ASCII capitals become lowercase. Input with a code
    /// checks only that handle. A name without a code checks the name and
    /// [`HANDLE_SEARCH_CODES`](inbox_handles::HANDLE_SEARCH_CODES) distinct random codes in one lookup, and the
    /// name comes first in the result. Text outside the handle grammar fails
    /// before any request. No policy applies here, so a returned handle can
    /// still be refused when it is claimed.
    pub async fn available_handles(&self, input: &str) -> Result<Vec<Handle>, ClientError> {
        Ok(inbox_handles::available_handles(&self.context, input).await?)
    }

    /// Claim `handle` for this client's inbox, signed by this installation's
    /// key. A claim of a handle the inbox already holds makes it the display
    /// handle again. The backend refuses a taken, reserved, short, or too
    /// frequent claim with `ApiError::HandleRejected`. After the publish, the
    /// client has stored the inbox's new identity update.
    pub async fn claim_handle(&self, handle: &Handle) -> Result<(), ClientError> {
        self.ensure_identity_ready::<ClientError>()?;
        Ok(inbox_handles::claim_handle(&self.context, handle).await?)
    }

    /// The inbox of each target, in order, from one call. Fails when any
    /// target has no inbox: an account as `find_or_create_dm_by_identity`
    /// does, and a handle with [`HandleError::NotFound`].
    pub async fn resolve_targets(
        &self,
        targets: &[IdentityTarget],
    ) -> Result<Vec<InboxId>, ClientError> {
        Ok(inbox_handles::resolve_targets(&self.context, targets).await?)
    }
}
