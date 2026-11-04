//! Client builder storage: the data location, the database store and the MLS storage.

use super::*;

pub(super) type ContextParts<Api, S, Db> = Arc<XmtpMlsLocalContext<Api, Db, S>>;
/// Opens the store at the paths `build` resolves for a storage location.
#[cfg(not(target_arch = "wasm32"))]
pub(super) type LocationStoreOpener<Db> = Box<
    dyn FnOnce(
            crate::storage_location::ResolvedPaths,
        ) -> xmtp_common::BoxDynFuture<'static, Result<Db, ClientBuilderError>>
        + Send
        + Sync,
>;
#[cfg(target_arch = "wasm32")]
pub(super) type LocationStoreOpener<Db> = Box<
    dyn FnOnce(
        crate::storage_location::ResolvedPaths,
    ) -> xmtp_common::BoxDynFuture<'static, Result<Db, ClientBuilderError>>,
>;

#[cfg(not(target_arch = "wasm32"))]
fn open_location_store(
    paths: crate::storage_location::ResolvedPaths,
    key: xmtp_db::EncryptionKey,
) -> xmtp_common::BoxDynFuture<'static, Result<xmtp_db::DefaultStore, ClientBuilderError>> {
    Box::pin(async move {
        let parent = paths
            .db_path
            .parent()
            .ok_or(crate::storage_location::StorageLocationError::InboxId)?;
        xmtp_attachments::create_private_directory(parent)
            .await
            .map_err(crate::storage_location::StorageLocationError::from)?;
        let db = xmtp_db::NativeDb::builder()
            .persistent(paths.db_path.to_string_lossy().into_owned())
            .key(key)
            .build()?;
        Ok(xmtp_db::EncryptedMessageStore::new(db)?)
    })
}

impl<ApiClient, S, Db> ClientBuilder<ApiClient, S, Db> {
    /// Save a deployment-scoped location. `build` resolves it after all options are set.
    ///
    /// Native only. Browser builds have no encrypted storage; call `data_location_with`
    /// and open the store with `WasmDb::new_strict`.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn data_location(
        self,
        location: crate::storage_location::StorageLocation,
        key: xmtp_db::EncryptionKey,
    ) -> Result<ClientBuilder<ApiClient, S, xmtp_db::DefaultStore>, ClientBuilderError> {
        self.data_location_with(location, move |paths| open_location_store(paths, key))
    }

    /// Save a deployment-scoped location that `opener` opens. `build` resolves
    /// the paths after all options are set and passes them to `opener`, so a
    /// host can open the store with its own connection options.
    pub fn data_location_with<NewDb>(
        self,
        location: crate::storage_location::StorageLocation,
        opener: impl FnOnce(
            crate::storage_location::ResolvedPaths,
        )
            -> xmtp_common::BoxDynFuture<'static, Result<NewDb, ClientBuilderError>>
        + xmtp_common::MaybeSend
        + xmtp_common::MaybeSync
        + 'static,
    ) -> Result<ClientBuilder<ApiClient, S, NewDb>, ClientBuilderError> {
        location.validate()?;
        let conflict = self.storage_location_conflict
            || self.store.is_some()
            || self.mls_storage.is_some()
            || self.mls_storage_factory.is_some()
            || self.storage_location_selected
            || self.attachments_dir.is_some();
        Ok(ClientBuilder {
            deployment_recorder: None,
            data_location: Some(location),
            location_store_opener: Some(Box::new(opener)),
            mls_storage_factory: None,
            storage_location_selected: true,
            storage_location_conflict: conflict,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
            api_client: self.api_client,
            identity: self.identity,
            store: None,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: self.application_components_for_test,
        })
    }

    pub fn store<NewDb>(self, db: NewDb) -> ClientBuilder<ApiClient, S, NewDb> {
        ClientBuilder {
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: None,
            mls_storage_factory: None,
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict
                || self.storage_location_selected,
            store: Some(db),
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self.mls_storage,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: self.application_components_for_test,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
        }
    }

    /// Use the default SQlite MLS Key-Value Store
    pub fn default_mls_store(
        self,
    ) -> Result<
        ClientBuilder<ApiClient, SqlKeyStore<<Db as XmtpDb>::DbQuery>, Db>,
        ClientBuilderError,
    >
    where
        Db: XmtpDb,
    {
        Ok(ClientBuilder {
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: self.location_store_opener,
            mls_storage_factory: Some(|store: &Db| SqlKeyStore::new(store.db())),
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict: self.storage_location_conflict,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: self
                .store
                .as_ref()
                .map(|store| SqlKeyStore::new(store.db())),
            store: self.store,
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: self.application_components_for_test,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
        })
    }

    pub fn mls_storage<NewS>(self, mls_storage: NewS) -> ClientBuilder<ApiClient, NewS, Db> {
        let storage_location_conflict =
            self.storage_location_conflict || self.storage_location_selected;
        ClientBuilder {
            deployment_recorder: self.deployment_recorder,
            data_location: self.data_location,
            location_store_opener: self.location_store_opener,
            mls_storage_factory: None,
            storage_location_selected: self.storage_location_selected,
            storage_location_conflict,
            store: self.store,
            api_client: self.api_client,
            identity: self.identity,
            identity_strategy: self.identity_strategy,
            scw_verifier: self.scw_verifier,
            custom_scw_verifier: self.custom_scw_verifier,
            device_sync_worker_mode: self.device_sync_worker_mode,
            fork_recovery_opts: self.fork_recovery_opts,
            change_callbacks: self.change_callbacks,
            stream_policy: self.stream_policy,
            incoming_factory: self.incoming_factory,
            version_info: self.version_info,
            allow_offline: self.allow_offline,
            disable_commit_log_worker: self.disable_commit_log_worker,
            mls_storage: Some(mls_storage),
            disable_workers: self.disable_workers,
            worker_config: self.worker_config,
            config_provider: self.config_provider,
            #[cfg(any(test, feature = "test-utils"))]
            application_components_for_test: self.application_components_for_test,
            attachments_dir: self.attachments_dir,
            attachment_options: self.attachment_options,
        }
    }
}
