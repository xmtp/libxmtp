//! Client registration: registering the identity, waiting for it to be visible, and
//! key package rotation.

use super::*;

const REGISTRATION_INITIAL_BACKOFF: std::time::Duration = std::time::Duration::from_millis(50);
const REGISTRATION_MAX_BACKOFF: std::time::Duration = std::time::Duration::from_secs(1);

impl<Context> Client<Context>
where
    Context: XmtpSharedContext,
{
    /// Register this installation with an inbox owner and wait until it is visible.
    pub async fn register_with_owner(&self, owner: &impl InboxOwner) -> Result<(), ClientError> {
        if self.identity().is_ready() {
            return self.ensure_registration_visible().await;
        }

        let mut signature_request = self
            .context
            .signature_request()
            .ok_or(SignatureRequestError::MissingSigner)?;
        let signature = owner
            .sign(&signature_request.signature_text())
            .map_err(SignatureError::from)?;
        signature_request
            .add_signature(signature, self.scw_verifier())
            .await?;
        self.register_identity(signature_request).await
    }

    /// Upload the key package before the identity update exposes this installation.
    /// Record its receipt for key retirement and retain the registration cursor.
    /// Return success only after the backend can read the registration receipt.
    #[xmtp_common::mls_span]
    pub async fn register_identity(
        &self,
        signature_request: SignatureRequest,
    ) -> Result<(), ClientError> {
        tracing::info!("registering identity");
        // Registration is a network call like any other.
        self.context.server_configuration().check()?;

        // Resume visibility confirmation after a stored registration.
        let stored_identity: Option<StoredIdentity> = self.context.db().fetch(&())?;
        if stored_identity.is_some() {
            tracing::info!("Resuming stored identity registration");
            self.identity().set_ready();
            return self.ensure_registration_visible().await;
        }

        // Step 1: Generate key package and store locally (not uploaded yet)
        let (kp_bytes, history_id) = crate::identity_lifecycle::generate_and_store_key_package(
            self.identity(),
            self.context.mls_storage(),
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )?;

        // Step 2: Validate signatures (fails here if invalid - no network pollution)
        let identity_update = signature_request
            .build_identity_update()
            .map_err(IdentityUpdateError::from)?;
        identity_update
            .to_verified(&self.context.scw_verifier())
            .await?;

        // Step 3: Upload key package first (prevents race condition)
        let key_package_meta = self.context.api().upload_key_package(kp_bytes).await?;
        let key_package_cursor = key_package_meta
            .cursor
            .filter(|cursor| cursor.sequence_id > 0)
            .ok_or(xmtp_api::ApiError::InvalidResponse(
                "key package publish cursor",
            ))?;

        // Own-installation changes are compared from before publication. A
        // conflict retry can load another installation's update into this
        // database before the final reload, and those changes are reported too.
        let inbox_id = self.inbox_id().to_string();
        let registration_cursor = self
            .identity_updates()
            .with_own_installation_events(
                &self.context.db(),
                Box::pin(async {
                    // Step 4: Publish identity update (makes installation visible)
                    let registration_cursor = crate::identity_updates::publish_with_conflict_retry(
                        self.context.api(),
                        &self.context.db(),
                        identity_update,
                        &self.context.scw_verifier(),
                    )
                    .await
                    .map_err(|error| IdentityUpdateError::Publish(Box::new(error)))?;
                    // Step 5: Fetch and store in local DB (needed for group operations)
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
                    )?;
                    Ok(registration_cursor)
                }),
            )
            .await?;

        // Backend publication order can differ from local generation order.
        crate::state_tx::state_write(self.context.mls_storage(), |tx| {
            let storage = tx.storage();
            storage.db().record_key_package_publication(
                history_id,
                xmtp_proto::types::Cursor(key_package_cursor.sequence_id),
            )?;
            storage
                .db()
                .reset_key_package_rotation_queue(KEY_PACKAGE_ROTATION_INTERVAL_NS)?;
            Ok::<_, StorageError>(Continue(()))
        })
        .map(TransactionOutcome::into_continued)?;

        // Mark identity as ready
        let mut stored_identity = StoredIdentity::try_from(self.identity())?;
        stored_identity.registration_cursor_sequence_id = Some(registration_cursor.0 as i64);
        stored_identity.store(&self.context.db())?;
        self.context
            .registration_event_pending()
            .store(true, Ordering::Release);
        self.identity().set_ready();
        self.ensure_registration_visible().await
    }

    /// Whether registration has completed its visibility confirmation.
    /// A stored identity with a pending receipt is not yet registered.
    pub fn is_registration_visible(&self) -> Result<bool, ClientError> {
        let stored: Option<StoredIdentity> = self.context.db().fetch(&())?;
        Ok(self.identity().is_ready()
            && stored.is_some_and(|identity| identity.registration_cursor_sequence_id.is_none()))
    }

    /// Confirm a stored registration before reporting success.
    pub async fn ensure_registration_visible(&self) -> Result<(), ClientError> {
        let stored: Option<StoredIdentity> = self.context.db().fetch(&())?;
        if stored.is_some_and(|identity| identity.registration_cursor_sequence_id.is_some()) {
            self.wait_for_registration_visible(VisibilityConfirmationOptions::default())
                .await?;
        }
        Ok(())
    }

    /// Wait until the serving database exposes the registration identity-topic head.
    /// A missing or older head is polled. The timeout also bounds each request.
    pub async fn wait_for_registration_visible(
        &self,
        options: VisibilityConfirmationOptions,
    ) -> Result<(), ClientError> {
        use xmtp_common::time::{Duration, sleep, timeout};
        if !self.identity().is_ready() {
            return Err(ClientError::RegistrationNotVisible);
        }
        let stored: Option<StoredIdentity> = self.context.db().fetch(&())?;
        let stored = stored.ok_or(ClientError::RegistrationNotVisible)?;
        let Some(receipt) = stored.registration_cursor_sequence_id else {
            return Ok(());
        };
        self.context.server_configuration().check()?;
        let sequence_id = u64::try_from(receipt)
            .ok()
            .filter(|sequence_id| *sequence_id != 0)
            .ok_or(ClientError::RegistrationNotVisible)?;
        timeout(Duration::from_millis(options.timeout_ms), async {
            let mut delay = REGISTRATION_INITIAL_BACKOFF;
            let inbox = hex::decode(self.inbox_id())
                .map_err(|_| xmtp_api::ApiError::InvalidRequest("registration inbox id"))?;
            let topic = xmtp_proto::types::Topic::new_identity_update(inbox);
            xmtp_proto::types::Topic::parse(&topic)?;
            loop {
                let heads = self
                    .context
                    .api()
                    .newest_topic_cursors(vec![topic.clone()])
                    .await?;
                let head = heads
                    .get(&topic)
                    .ok_or(xmtp_api::ApiError::InvalidResponse(
                        "registration identity head",
                    ))?;
                // implements: IDENT-072
                if head.0 >= sequence_id {
                    return Ok::<(), ClientError>(());
                }
                sleep(delay).await;
                delay = (delay * 2).min(REGISTRATION_MAX_BACKOFF);
            }
        })
        .await
        .map_err(|_| ClientError::RegistrationNotVisible)??;
        if self
            .context
            .registration_event_pending()
            .swap(false, Ordering::AcqRel)
        {
            xmtp_events::EventWriter::emit(
                self.context.events(),
                Some(xmtp_events::ClientEvent::IdentityRegistered(
                    xmtp_events::IdentityRegistered {
                        inbox_id: self.inbox_id().to_string(),
                        installation_key: self.installation_id.to_vec(),
                    },
                )),
                None,
            );
        }
        self.context.db().clear_registration_cursor(receipt)?;
        Ok(())
    }

    /// If no key rotation is scheduled, queue it to occur in the next 5 seconds.
    pub fn queue_key_rotation(&self) -> Result<(), ClientError> {
        crate::worker::key_package_maintenance::queue_key_rotation(&self.context)?;
        Ok(())
    }

    /// Upload a new key package to the network replacing an existing key package.
    pub async fn rotate_and_upload_key_package(&self) -> Result<(), ClientError> {
        // A client with a blocked connection publishes nothing. This one
        // reaches `Identity` directly instead of going through a group or the
        // publish path, so it carries its own gate.
        self.ensure_identity_ready::<ClientError>()?;
        crate::identity_lifecycle::rotate_and_upload_key_package(
            self.identity(),
            self.context.api(),
            self.context.mls_storage(),
            CREATE_PQ_KEY_PACKAGE_EXTENSION,
        )
        .await?;
        // The rotation marked superseded KPs delete_at=now+grace; without this
        // the parked KpDeletion task would sweep them up to ~30d late.
        crate::worker::key_package_maintenance::nudge_deletion(&self.context)?;

        Ok(())
    }

    /// Fetches the current key package from the network for each of the `installation_id`s specified
    #[tracing::instrument(skip_all)]
    pub async fn get_key_packages_for_installation_ids(
        &self,
        installation_ids: Vec<Vec<u8>>,
    ) -> Result<KeyPackageResultsByInstallation, ClientError> {
        MlsStore::new(self.context.clone())
            .get_key_packages_for_installation_ids(installation_ids)
            .await
            .map_err(Into::into)
    }
}
