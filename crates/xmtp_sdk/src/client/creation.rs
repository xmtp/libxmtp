use super::*;
use xmtp_mls::identity_updates::{identifier_membership, identifier_opens_inbox};

/// Retain cleanup ownership while registration can be cancelled.
struct RegisteringClient {
    client: Option<Client>,
    #[cfg(not(target_arch = "wasm32"))]
    runtime: tokio::runtime::Handle,
}

impl RegisteringClient {
    fn new(client: Client) -> Self {
        Self {
            client: Some(client),
            #[cfg(not(target_arch = "wasm32"))]
            runtime: tokio::runtime::Handle::current(),
        }
    }

    fn take(mut self) -> Client {
        self.client.take().expect("registering client")
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for RegisteringClient {
    fn drop(&mut self) {
        if let Some(client) = self.client.take() {
            self.runtime.spawn(async move {
                let _ = client.discard().await;
            });
        }
    }
}

impl Client {
    /// End a client that a failed create does not return. The built client
    /// already runs background work on its store. The browser host releases
    /// the storage lock when create fails, so the store must not stay
    /// connected, even when close fails and keeps it connected for a retry.
    /// Returns an error when the store stays connected. Then
    /// `storage_requires_worker_restart` reports it to the browser worker, which keeps the
    /// storage lock.
    pub(crate) async fn discard(&self) -> Result<(), XmtpError> {
        let Err(error) = self.end().await else {
            return Ok(());
        };
        tracing::warn!(%error, "closing the client of a failed create");
        let disconnected = self.disconnect_discarded();
        #[cfg(target_arch = "wasm32")]
        xmtp_db::pause_sqlite_if_idle();
        if let Err(error) = &disconnected {
            tracing::error!(%error, "the store of a failed create stays connected");
            #[cfg(any(test, target_arch = "wasm32"))]
            STORE_LEFT_OPEN.store(true, Ordering::Relaxed);
        }
        disconnected
    }

    fn disconnect_discarded(&self) -> Result<(), XmtpError> {
        use xmtp_db::ConnectionExt;

        #[cfg(test)]
        if FAIL_DISCARD_DISCONNECT.swap(false, Ordering::Relaxed) {
            return Err(XmtpError::storage("injected disconnect failure"));
        }
        self.inner
            .context
            .db()
            .disconnect()
            .map_err(XmtpError::from_core)
    }

    pub(super) async fn build_inner(
        identity: PublicIdentity,
        options: ClientOptions,
        inbox_id: Option<InboxId>,
        require_stored_identity: bool,
        guard: &mut OpenStoreGuard,
    ) -> Result<Self, XmtpError> {
        let built =
            Self::build_client(identity, options, inbox_id, require_stored_identity, guard).await;
        // A failed build can unpause the OPFS pool and then drop its store,
        // for example when the database has no stored identity. The browser
        // host releases the storage lock after the failure, so the pool must
        // not keep its access handles. A live database keeps the pool open.
        #[cfg(target_arch = "wasm32")]
        if built.is_err() {
            xmtp_db::pause_sqlite_if_idle();
        }
        built
    }

    async fn build_client(
        identity: PublicIdentity,
        options: ClientOptions,
        inbox_id: Option<InboxId>,
        require_stored_identity: bool,
        guard: &mut OpenStoreGuard,
    ) -> Result<Self, XmtpError> {
        use xmtp_mls::storage_location::StorageLocation as CoreLocation;

        let inbox_id = inbox_id.map(InboxId::into_checked).transpose()?;
        let fork_recovery = options.fork_recovery_opts()?;
        let location = super::location::core_location(&options.storage)?;
        let identifier = identity.to_core()?;
        let nonce = options.registration.nonce.unwrap_or(0);
        // An explicit database holds its inbox ID. Open it before any request,
        // so an offline reopen needs no inbox ID.
        let mut opened = None;
        let mut stored_inbox = None;
        if let Some(CoreLocation::Explicit { db_path, .. }) = &location {
            guard.arm(&options.storage);
            match open_store_if_present(&options.storage, db_path).await? {
                Some((store, stored)) => {
                    if require_stored_identity && stored.is_none() {
                        return Err(XmtpError::identity_not_found());
                    }
                    // Keep the stored inbox ID for the identity check, even
                    // when the caller supplies an inbox ID.
                    stored_inbox = stored;
                    opened = Some(super::location::OpenedStore {
                        path: db_path.clone(),
                        store,
                    });
                }
                None if require_stored_identity => return Err(XmtpError::identity_not_found()),
                None => {}
            }
        }
        if options.allow_offline && inbox_id.is_none() && stored_inbox.is_none() {
            return Err(XmtpError::invalid("allowOffline requires an inbox ID"));
        }
        if require_stored_identity && location.is_none() {
            return Err(XmtpError::identity_not_found());
        }
        if stored_inbox
            .as_ref()
            .zip(inbox_id.as_ref())
            .is_some_and(|(stored, supplied)| stored != supplied)
        {
            return Err(XmtpError::identity_mismatch());
        }
        let backend = options
            .backend
            .clone()
            .unwrap_or_default()
            .resolve()
            .await?;
        let auth_handle = backend.auth_handle.clone();
        // Offline, check an explicit store with local identity updates. A
        // smart contract wallet state that needs a request is not known.
        let offline_stored_checked = if options.allow_offline
            && let Some((stored, opened)) = stored_inbox.as_ref().zip(opened.as_ref())
        {
            use xmtp_id::associations::SignatureError;
            use xmtp_mls::client::ClientError;
            let membership = match identifier_membership(
                &opened.store.db(),
                stored,
                &identifier,
                &NoRequestVerifier,
            )
            .await
            {
                Ok(membership) => membership,
                Err(ClientError::SignatureValidation(SignatureError::VerifierError(_))) => {
                    return Err(XmtpError::identity_mismatch());
                }
                Err(error) => return Err(XmtpError::from_client(error)),
            };
            if !identifier_opens_inbox(membership, &identifier, nonce, stored) {
                return Err(XmtpError::identity_mismatch());
            }
            true
        } else {
            false
        };
        // Whether the build checks that the identifier belongs to the inbox it
        // opens, after it has checked the deployment and before any worker.
        let mut check_membership = false;
        let strategy = |inbox_id| IdentityStrategy::new(inbox_id, identifier.clone(), nonce);
        let strategy = match (inbox_id, stored_inbox.zip(opened.as_ref())) {
            (Some(value), _) => {
                check_membership = !offline_stored_checked;
                strategy(value)
            }
            (None, Some((stored, _))) if options.allow_offline => strategy(stored),
            // Online, the identity updates come from the backend, which the
            // build checks against the stored deployment before any identity
            // request. The build checks the identifier on them before it
            // starts any worker.
            (None, Some((stored, _))) => {
                check_membership = true;
                strategy(stored)
            }
            // The build looks up the identifier's inbox after it checks the
            // deployment, because the request carries the identifier. With
            // no live inbox, it falls back to the one the identifier created,
            // which a data directory may already store. The core build
            // trusts that stored identity, so it checks the identifier too.
            (None, None) => {
                check_membership = true;
                IdentityStrategy::for_identifier(identifier.clone(), nonce)
            }
        };
        let mode = if options.device_sync {
            DeviceSyncMode::Enabled
        } else {
            DeviceSyncMode::Disabled
        };
        let builder = xmtp_mls::Client::builder(strategy)
            .api_client_with_streams(backend.api.clone())
            .with_allow_offline(Some(options.allow_offline))
            .with_remote_verifier()
            .map_err(XmtpError::from_core)?
            .attachment_options(
                options
                    .attachments
                    .clone()
                    .map(Into::into)
                    .unwrap_or_default(),
            );
        let outcome = Arc::new(parking_lot::Mutex::new(
            super::location::OpenOutcome::default(),
        ));
        let mut builder = match location {
            None => builder.store(open_store(&options.storage, None).await?),
            Some(location) => {
                guard.arm(&options.storage);
                builder
                    .data_location_with(
                        location,
                        super::location::store_opener(
                            options.storage.clone(),
                            opened,
                            require_stored_identity,
                            outcome.clone(),
                        ),
                    )
                    .map_err(XmtpError::from_builder)?
            }
        }
        .device_sync_worker_mode(mode);
        if check_membership {
            builder = builder.require_identifier_in_inbox();
        }
        if let Some(recovery) = fork_recovery {
            builder = builder.fork_recovery_opts(recovery);
        }
        if let Some(workers) = options.workers.clone() {
            builder = builder.worker_config(workers.into());
        }
        #[cfg(all(test, not(target_arch = "wasm32")))]
        if let Some(components) = crate::metadata::catalogue_override::application_components() {
            builder = builder.application_components_for_test(components);
        }
        let built = builder
            .default_mls_store()
            .map_err(XmtpError::from_core)?
            .build()
            .await;
        let (storage_path, open_error) = {
            let mut outcome = outcome.lock();
            (outcome.path.take(), outcome.error.take())
        };
        let inner =
            built.map_err(|error| open_error.unwrap_or_else(|| XmtpError::from_builder(error)))?;
        let key = NEXT_CLIENT_KEY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |key| {
                key.checked_add(1)
            })
            .map_err(|_| XmtpError::unknown("client key space exhausted"))?;
        let client = Self {
            inner: Arc::new(inner),
            key,
            identity,
            options,
            storage_path,
            signer: None,
            auth_handle,
            listeners: Arc::new(crate::events::dispatch::ListenerRegistry::default()),
            event_readers: Arc::new(parking_lot::Mutex::new(EventReaderRegistry::default())),
            #[cfg(test)]
            call_gate: parking_lot::Mutex::new(None),
        };
        Ok(client)
    }
}

/// Fails every smart contract wallet check, because an offline build sends no
/// request.
struct NoRequestVerifier;

#[xmtp_common::async_trait]
impl xmtp_id::scw_verifier::SmartContractSignatureVerifier for NoRequestVerifier {
    async fn is_valid_signature(
        &self,
        _account_id: AccountId,
        _hash: [u8; 32],
        _signature: alloy::primitives::Bytes,
        _block_number: Option<alloy::primitives::BlockNumber>,
    ) -> Result<xmtp_id::scw_verifier::ValidationResponse, xmtp_id::scw_verifier::VerifierError>
    {
        Err(std::io::Error::other("an offline build checks no smart contract wallet").into())
    }
}

impl Client {
    pub(super) async fn create_with_guard(
        signer: Arc<dyn Signer>,
        identity: PublicIdentity,
        options: ClientOptions,
        guard: &mut OpenStoreGuard,
    ) -> Result<Self, XmtpError> {
        let client = Self::build_inner(identity, options, None, false, guard).await?;
        #[cfg(all(test, not(target_arch = "wasm32")))]
        let _ = build_task_probe::CURRENT.try_with(|probe| {
            *probe.client.lock() = Some(client.inner.clone());
        });
        let mut registering = RegisteringClient::new(client);
        let client = registering.client.as_mut().expect("registering client");
        if client.options.registration.auto {
            let registered = match signer::kind(signer.clone()).await {
                Ok(kind) => client.register_with_signer(signer.clone(), kind).await,
                Err(error) => Err(error),
            };
            if let Err(error) = registered {
                // The caller gets the registration error. A store that stays
                // open is reported through `storage_requires_worker_restart`.
                let _ = client.discard().await;
                registering.take();
                return Err(error);
            }
        }
        client.signer = Some(signer);
        Ok(registering.take())
    }

    pub(crate) async fn register_with_signer(
        &self,
        signer: Arc<dyn Signer>,
        kind: SignerKind,
    ) -> Result<(), XmtpError> {
        let Some(mut request) = self.inner.identity().signature_request() else {
            return Ok(());
        };
        if let Some(handler) = self
            .options
            .handlers
            .as_ref()
            .and_then(|handlers| handlers.pre_authenticate.clone())
        {
            crate::foreign::call(async move { handler.run().await })
                .await
                .map_err(|_| XmtpError::callback_failed())?
                .map_err(|_| XmtpError::callback_failed())?;
        }
        let signature = signer::sign(
            signer,
            SigningRequest {
                text: request.signature_text(),
            },
        )
        .await?;
        let verifier = self.inner.scw_verifier();
        match (kind, signature) {
            (SignerKind::Eoa, Signature::Ecdsa(bytes)) => {
                request
                    .add_signature(UnverifiedSignature::new_recoverable_ecdsa(bytes), &verifier)
                    .await
                    .map_err(XmtpError::from_signature_request)?;
            }
            (
                SignerKind::Passkey,
                Signature::Passkey {
                    signature,
                    public_key,
                    authenticator_data,
                    client_data_json,
                },
            ) => {
                request
                    .add_signature(
                        UnverifiedSignature::new_passkey(
                            public_key,
                            signature,
                            authenticator_data,
                            client_data_json,
                        ),
                        &verifier,
                    )
                    .await
                    .map_err(XmtpError::from_signature_request)?;
            }
            (
                SignerKind::Scw { chain_id, .. },
                Signature::Scw {
                    bytes,
                    address,
                    chain_id: signed_chain_id,
                    block_number: signed_block,
                },
            ) if chain_id == signed_chain_id => {
                request
                    .add_new_unverified_smart_contract_signature(
                        NewUnverifiedSmartContractWalletSignature::new(
                            bytes,
                            AccountId::new_evm(chain_id, address),
                            signed_block,
                        ),
                        &verifier,
                    )
                    .await
                    .map_err(XmtpError::from_signature_request)?;
            }
            _ => return Err(XmtpError::invalid("signature does not match signer kind")),
        }
        let _call = self.ensure_open()?;
        self.inner
            .register_identity(request)
            .await
            .map_err(XmtpError::from_client)
    }
}
