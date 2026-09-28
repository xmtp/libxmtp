use super::*;

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
            .map_err(XmtpError::storage)
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
        mut options: ClientOptions,
        inbox_id: Option<InboxId>,
        require_stored_identity: bool,
        guard: &mut OpenStoreGuard,
    ) -> Result<Self, XmtpError> {
        if matches!(&options.storage.location, StorageLocation::Default) {
            return Err(XmtpError::storage_location_required());
        }
        #[cfg(not(target_arch = "wasm32"))]
        match &mut options.storage.location {
            StorageLocation::Path(path) | StorageLocation::Directory(path) => {
                *path = std::path::absolute(&*path)
                    .map_err(XmtpError::unknown)?
                    .to_string_lossy()
                    .into_owned();
            }
            StorageLocation::Default | StorageLocation::InMemory => {}
        }
        if options.allow_offline && inbox_id.is_none() {
            return Err(XmtpError::invalid("allowOffline requires an inbox ID"));
        }
        let identifier = identity.to_core()?;
        // Check a known database before resolving the backend. Build must not
        // fetch configuration or create an identity for an empty database.
        let checked_store = match (require_stored_identity, inbox_id.as_ref()) {
            (true, Some(inbox_id)) => {
                guard.arm(&options.storage);
                Some(open_existing_store(&options.storage, &inbox_id.0).await?)
            }
            _ => None,
        };
        let backend = options
            .backend
            .clone()
            .unwrap_or_default()
            .resolve()
            .await?;
        let auth_handle = backend.auth_handle.clone();
        let inbox_id = match inbox_id {
            Some(value) => value.0,
            None => {
                let api = xmtp_api::ApiClientWrapper::new(backend.api.clone(), Default::default());
                let found = api
                    .get_inbox_ids(vec![identifier.clone().into()])
                    .await
                    .map_err(XmtpError::from_api)?;
                match found.into_iter().next().flatten() {
                    Some(value) => value,
                    None => identifier
                        .inbox_id(options.registration.nonce.unwrap_or(0))
                        .map_err(XmtpError::unknown)?,
                }
            }
        };
        guard.arm(&options.storage);
        let store = match checked_store {
            Some(store) => store,
            None if require_stored_identity => {
                open_existing_store(&options.storage, &inbox_id).await?
            }
            None => open_store(&options.storage, &inbox_id).await?,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let storage_path = native_storage_path(&options.storage, &inbox_id)?;
        #[cfg(target_arch = "wasm32")]
        let storage_path = wasm_storage_path(&options.storage, &inbox_id)?;
        let mode = if options.device_sync {
            DeviceSyncMode::Enabled
        } else {
            DeviceSyncMode::Disabled
        };
        let mut builder = xmtp_mls::Client::builder(IdentityStrategy::new(
            inbox_id,
            identifier,
            options.registration.nonce.unwrap_or(0),
            None,
        ))
        .api_client_with_streams(backend.api.clone())
        .with_allow_offline(Some(options.allow_offline))
        .with_remote_verifier()
        .map_err(XmtpError::unknown)?
        .store(store)
        .device_sync_worker_mode(mode);
        if let Some(recovery) = options.fork_recovery.clone() {
            builder = builder.fork_recovery_opts(recovery.into());
        }
        if let Some(workers) = options.workers.clone() {
            builder = builder.worker_config(workers.into());
        }
        let inner = builder
            .default_mls_store()
            .map_err(XmtpError::unknown)?
            .build()
            .await
            .map_err(XmtpError::from_builder)?;
        let key = NEXT_CLIENT_KEY
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |key| {
                key.checked_add(1)
            })
            .map_err(|_| XmtpError::unknown("client key space exhausted"))?;
        Ok(Self {
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
        })
    }

    pub(super) async fn create_with_guard(
        signer: Arc<dyn Signer>,
        identity: PublicIdentity,
        options: ClientOptions,
        guard: &mut OpenStoreGuard,
    ) -> Result<Self, XmtpError> {
        let mut client = Self::build_inner(identity, options, None, false, guard).await?;
        if client.options.registration.auto {
            let registered = match signer::kind(signer.clone()).await {
                Ok(kind) => client.register_with_signer(signer.clone(), kind).await,
                Err(error) => Err(error),
            };
            if let Err(error) = registered {
                // The caller gets the registration error. A store that stays
                // open is reported through `storage_requires_worker_restart`.
                let _ = client.discard().await;
                return Err(error);
            }
        }
        client.signer = Some(signer);
        Ok(client)
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
