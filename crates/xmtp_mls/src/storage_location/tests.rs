use super::*;

// verifies: ATCH-040
#[xmtp_common::test(unwrap_try = true)]
fn deployment_dirs_do_not_collide() {
    let values = ["acme/prod", "beta/prod", "Prod", "prod", "a:b", "ab"];
    let names: Vec<_> = values
        .iter()
        .map(|value| deployment_component(value))
        .collect();
    let distinct: std::collections::HashSet<_> = names.iter().collect();
    assert_eq!(distinct.len(), values.len());
    assert!(names.iter().all(|name| name.len() <= 255));
    assert!(
        names
            .iter()
            .all(|name| name.bytes().all(|b| b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || b == b'-'
                || b == b'.'
                || b == b'_'))
    );
    let long = deployment_component(&"x".repeat(256));
    assert_eq!(long.len(), 255);
    assert_eq!(long.split('-').next().map(str::len), Some(190));
}

// verifies: ATCH-040
#[xmtp_common::test(unwrap_try = true)]
fn database_name_uses_deployment_and_inbox() {
    let inbox = "A1B2C3";
    let location = StorageLocation::DataDir(PathBuf::from("client-data"));
    let paths = location.resolve_identifier(inbox, "production")?;
    let name = paths.db_path.to_string_lossy().into_owned();
    let expected = format!(
        "client-data/{}/a1b2c3/xmtp.db3",
        deployment_component("production")
    );
    assert_eq!(name, expected);
    let option = xmtp_db::StorageOption::Persistent(name);
    assert!(matches!(option, xmtp_db::StorageOption::Persistent(ref path) if path == &expected));
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::{Client, InboxOwner, utils::test::identity_setup};
    use prost::Message;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::prelude::QueryServerConfiguration;
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;

    fn builder() -> crate::builder::ClientBuilder<xmtp_api_backend::MockBackendClient, ()> {
        let owner = generate_local_wallet();
        let api = xmtp_api_backend::MockBackendClient::new();
        Client::builder(identity_setup(owner))
            .api_client(api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    }

    // verifies: ATCH-082
    #[xmtp_common::test(unwrap_try = true)]
    async fn missing_data_dir_is_rejected_before_io() {
        let root = tempfile::tempdir()?;
        let result = builder()
            .data_location(StorageLocation::DataDir(PathBuf::new()), [0u8; 32].into())
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::MissingPath { field: "data_dir" }
            ))
        ));
        assert!(std::fs::read_dir(root.path())?.next().is_none());
    }

    // verifies: ATCH-082
    #[xmtp_common::test(unwrap_try = true)]
    async fn missing_explicit_db_path_is_rejected_before_io() {
        let root = tempfile::tempdir()?;
        let result = builder()
            .data_location(
                StorageLocation::Explicit {
                    db_path: PathBuf::new(),
                    attachments_dir: root.path().join("attachments"),
                },
                [0u8; 32].into(),
            )
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::MissingPath { field: "db_path" }
            ))
        ));
        assert!(std::fs::read_dir(root.path())?.next().is_none());
    }

    // verifies: ATCH-082
    #[xmtp_common::test(unwrap_try = true)]
    async fn missing_explicit_attachments_dir_is_rejected_before_io() {
        let root = tempfile::tempdir()?;
        let result = builder()
            .data_location(
                StorageLocation::Explicit {
                    db_path: root.path().join("client.db3"),
                    attachments_dir: PathBuf::new(),
                },
                [0u8; 32].into(),
            )
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::MissingPath {
                    field: "attachments_dir"
                }
            ))
        ));
        assert!(std::fs::read_dir(root.path())?.next().is_none());
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn data_dir_without_backend_url_fails_before_request() {
        let dir = tempfile::tempdir()?;
        let mut builder = builder();
        builder
            .api_client
            .as_mut()
            .unwrap()
            .expect_get_configuration()
            .times(0);
        let result = builder
            .data_location(
                StorageLocation::DataDir(dir.path().to_path_buf()),
                [0u8; 32].into(),
            )
            .await?
            .default_mls_store()?
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::BackendUrl
            ))
        ));
        assert!(!dir.path().join("deployments.json").exists());
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn offline_uses_record() {
        let dir = tempfile::tempdir()?;
        // A second client opens the same database after its backend stops.
        use crate::utils::test::backend::EphemeralBackend;
        let backend = EphemeralBackend::start("").await?;
        let url = backend.url().to_owned();
        let location = StorageLocation::DataDir(dir.path().join("restart"));
        let owner = generate_local_wallet();
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(&url);
        let first = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        let inbox = first.inbox_id().to_string();
        let mut request = first.context.signature_request().unwrap();
        let signature = owner.sign(&request.signature_text())?;
        request
            .add_signature(signature, &MockSmartContractSignatureVerifier::new(true))
            .await?;
        first.register_identity(request).await?;
        drop(first);
        backend.stop().await?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(&url);
        let second = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_allow_offline(Some(true))
            .data_location(location, [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(second.inbox_id(), inbox);
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn miss_fetches_and_records() {
        use crate::utils::test::backend::EphemeralBackend;
        let dir = tempfile::tempdir()?;
        let backend = EphemeralBackend::start("").await?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        let client = Client::builder(identity_setup(generate_local_wallet()))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        let identifier = client.server_configuration().identifier.clone();
        assert!(
            client
                .context
                .attachments
                .dir
                .as_ref()
                .unwrap()
                .to_string_lossy()
                .contains(&deployment_component(&identifier))
        );
        assert_eq!(
            location.recorder(backend.url()).unwrap().lookup().await?,
            Some(identifier.clone())
        );
        let document: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(dir.path().join("deployments.json")).await?)?;
        assert_eq!(document["version"], 1);
        assert_eq!(document["deployments"][backend.url()], identifier);
    }

    // verifies: ATCH-040
    #[xmtp_common::test(unwrap_try = true)]
    async fn host_opener_opens_resolved_paths() {
        use crate::utils::test::backend::EphemeralBackend;
        let dir = tempfile::tempdir()?;
        let backend = EphemeralBackend::start("").await?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let opened = Arc::new(parking_lot::Mutex::new(None));
        let seen = opened.clone();
        let client = Client::builder(identity_setup(generate_local_wallet()))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location_with(
                StorageLocation::DataDir(dir.path().to_path_buf()),
                move |paths: ResolvedPaths| -> xmtp_common::BoxDynFuture<'static, _> {
                    Box::pin(async move {
                        std::fs::create_dir_all(paths.db_path.parent().unwrap())
                            .map_err(StorageLocationError::from)?;
                        let db = xmtp_db::NativeDb::builder()
                            .persistent(paths.db_path.to_string_lossy().into_owned())
                            .build_unencrypted()?;
                        *seen.lock() = Some(paths);
                        Ok(xmtp_db::EncryptedMessageStore::new(db)?)
                    })
                },
            )?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        let root = dir
            .path()
            .join(deployment_component(
                &client.server_configuration().identifier,
            ))
            .join(client.inbox_id().to_ascii_lowercase());
        let paths = opened
            .lock()
            .clone()
            .expect("the host opener opened the store");
        assert_eq!(paths.db_path, root.join("xmtp.db3"));
        assert_eq!(paths.attachments_dir, root.join("attachments"));
        assert!(paths.db_path.exists());
        assert_eq!(client.context.attachments.dir, Some(paths.attachments_dir));
    }

    // Covers plan P19.
    #[xmtp_common::test(unwrap_try = true)]
    async fn torn_file_treated_empty() {
        use crate::utils::test::backend::EphemeralBackend;
        let dir = tempfile::tempdir()?;
        tokio::fs::write(
            dir.path().join("deployments.json"),
            b"{\"version\":1,\"deployments\":{",
        )
        .await?;
        let backend = EphemeralBackend::start("").await?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        let client = Client::builder(identity_setup(generate_local_wallet()))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(
            location.recorder(backend.url()).unwrap().lookup().await?,
            Some(client.server_configuration().identifier.clone())
        );
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn offline_first_start_without_record_fails() {
        let dir = tempfile::tempdir()?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(format!("http://{}", listener.local_addr()?));
        let result = Client::builder(identity_setup(generate_local_wallet()))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_allow_offline(Some(true))
            .data_location(
                StorageLocation::DataDir(dir.path().to_path_buf()),
                [0u8; 32].into(),
            )
            .await?
            .default_mls_store()?
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::OfflineMissingDeployment
            ))
        ));
        assert!(!dir.path().join("deployments.json").exists());
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn offline_set_after_location_still_fails_without_request() {
        let dir = tempfile::tempdir()?;
        let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(format!("http://{}", listener.local_addr()?));
        let result = Client::builder(identity_setup(generate_local_wallet()))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(
                StorageLocation::DataDir(dir.path().to_path_buf()),
                [0u8; 32].into(),
            )
            .await?
            .with_allow_offline(Some(true))
            .default_mls_store()?
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::OfflineMissingDeployment
            ))
        ));
        assert!(!dir.path().join("deployments.json").exists());
        assert!(
            matches!(listener.accept(), Err(error) if error.kind() == std::io::ErrorKind::WouldBlock)
        );
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn cached_only_cannot_resolve_a_location() {
        let dir = tempfile::tempdir()?;
        let mut api = xmtp_api_backend::MockBackendClient::new();
        api.expect_get_configuration().times(0);
        let result = Client::builder(crate::identity::IdentityStrategy::CachedOnly)
            .api_client(api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(
                StorageLocation::DataDir(dir.path().to_path_buf()),
                [0u8; 32].into(),
            )
            .await?
            .default_mls_store()?
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::InboxId
            ))
        ));
    }

    // verifies: ATCH-080
    #[xmtp_common::test(unwrap_try = true)]
    async fn explicit_location_cached_only_empty_db_needs_identity() {
        let dir = tempfile::tempdir()?;
        let mut api = xmtp_api_backend::MockBackendClient::new();
        api.expect_get_configuration().times(0);
        let result = Client::builder(crate::identity::IdentityStrategy::CachedOnly)
            .api_client(api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::edited(|_| {}),
            ))
            .data_location(
                StorageLocation::Explicit {
                    db_path: dir.path().join("client.db3"),
                    attachments_dir: dir.path().join("attachments"),
                },
                [0u8; 32].into(),
            )
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::Identity(
                crate::identity::IdentityError::RequiredIdentityNotFound
            ))
        ));
    }

    // verifies: ATCH-080
    #[xmtp_common::test(unwrap_try = true)]
    async fn explicit_location_cached_only_reopens_same_inbox_without_request() {
        use crate::utils::test::backend::EphemeralBackend;
        let dir = tempfile::tempdir()?;
        let location = StorageLocation::Explicit {
            db_path: dir.path().join("client.db3"),
            attachments_dir: dir.path().join("attachments"),
        };
        let owner = generate_local_wallet();
        let backend = EphemeralBackend::start("").await?;
        let mut first_api = xmtp_api_backend::MessageBackendBuilder::new();
        first_api.host(backend.url());
        let first = Client::builder(identity_setup(&owner))
            .api_client_with_streams(first_api.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::edited(|_| {}),
            ))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        let inbox = first.inbox_id().to_owned();
        let mut request = first.context.signature_request().unwrap();
        let signature = owner.sign(&request.signature_text())?;
        request
            .add_signature(signature, &MockSmartContractSignatureVerifier::new(true))
            .await?;
        first.register_identity(request).await?;
        drop(first);
        backend.stop().await?;

        let mut second_api = xmtp_api_backend::MockBackendClient::new();
        second_api.expect_get_configuration().times(0);
        let second = Client::builder(crate::identity::IdentityStrategy::CachedOnly)
            .api_client(second_api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_allow_offline(Some(true))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::edited(|_| {}),
            ))
            .data_location(location, [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(second.inbox_id(), inbox);
    }

    // verifies: ATCH-040
    #[xmtp_common::test(unwrap_try = true)]
    async fn data_location_rejects_store_and_attachment_dir() {
        let dir = tempfile::tempdir()?;
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        let mut with_store = builder();
        with_store
            .api_client
            .as_mut()
            .unwrap()
            .expect_get_configuration()
            .times(0);
        let result = with_store
            .store(())
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::ConflictingStore
            ))
        ));

        let mut with_dir = builder();
        with_dir
            .api_client
            .as_mut()
            .unwrap()
            .expect_get_configuration()
            .times(0);
        let result = with_dir
            .attachments_dir(dir.path().join("custom"))
            .data_location(location, [0u8; 32].into())
            .await?
            .default_mls_store()?
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::ConflictingStore
            ))
        ));
    }

    // verifies: ATCH-040, ATCH-080
    #[xmtp_common::test(unwrap_try = true)]
    async fn data_location_rejects_custom_mls_storage_in_both_orders() {
        use xmtp_db::XmtpTestDb as _;

        let dir = tempfile::tempdir()?;
        let store = xmtp_db::TestDb::create_persistent_store(None).await;
        let custom = xmtp_db::sql_key_store::SqlKeyStore::new(store.db());
        let location = StorageLocation::DataDir(dir.path().join("data"));
        let mut api = xmtp_api_backend::MessageBackendBuilder::new();
        api.host("http://127.0.0.1:1");
        let first = Client::builder(identity_setup(generate_local_wallet()))
            .api_client_with_streams(api.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_allow_offline(Some(true))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .mls_storage(custom.clone())
            .build()
            .await;
        assert!(matches!(
            first,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::ConflictingStore
            ))
        ));

        let mut api = xmtp_api_backend::MessageBackendBuilder::new();
        api.host("http://127.0.0.1:1");
        let second = Client::builder(identity_setup(generate_local_wallet()))
            .api_client_with_streams(api.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_allow_offline(Some(true))
            .mls_storage(custom)
            .data_location(location, [0u8; 32].into())
            .await?
            .build()
            .await;
        assert!(matches!(
            second,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::ConflictingStore
            ))
        ));
    }

    // verifies: ATCH-069, CONF-040
    #[xmtp_common::test(unwrap_try = true)]
    async fn refresh_records_a_stored_answer() {
        use crate::utils::test::backend::EphemeralBackend;
        let dir = tempfile::tempdir()?;
        let backend = EphemeralBackend::start("").await?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let client = Client::builder(identity_setup(generate_local_wallet()))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(
                StorageLocation::DataDir(dir.path().to_path_buf()),
                [0u8; 32].into(),
            )
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        tokio::fs::remove_file(dir.path().join("deployments.json")).await?;
        client.refresh_server_configuration().await?;
        assert_eq!(
            DeploymentRecorder::new(dir.path().to_path_buf(), backend.url())
                .lookup()
                .await?,
            Some(client.server_configuration().identifier.clone())
        );
    }

    // verifies: ATCH-040, ATCH-069, CONF-026
    #[xmtp_common::test(unwrap_try = true)]
    async fn build_keeps_deployment_record_aligned_with_opened_paths() {
        use crate::utils::test::backend::EphemeralBackend;
        let dir = tempfile::tempdir()?;
        let backend = EphemeralBackend::start("").await?;
        let owner = generate_local_wallet();
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let first = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        let db = first.context.store.db();
        let row = db.server_configuration()?.unwrap();
        let first_inbox = first.inbox_id().to_owned();
        let original_identifier = row.identifier.clone();
        let mut response =
            xmtp_proto::backend_v1::GetConfigurationResponse::decode(row.response.as_slice())?;
        response.identifier = "new-deployment".to_owned();
        db.store_server_configuration(
            &response.identifier,
            &row.backend_url,
            &response.encode_to_vec(),
            xmtp_common::time::now_ns(),
        )?;
        drop(first);
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let second = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_allow_offline(Some(true))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await;
        assert!(matches!(
            second,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::DeploymentMismatch
            ))
        ));
        assert_eq!(
            location.recorder(backend.url()).unwrap().lookup().await?,
            Some(original_identifier.clone())
        );
        let wrong = location.resolve_identifier(&first_inbox, "new-deployment")?;
        assert!(!wrong.db_path.exists());

        db.store_server_configuration(
            &original_identifier,
            &row.backend_url,
            &row.response,
            xmtp_common::time::now_ns(),
        )?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let third = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_allow_offline(Some(true))
            .data_location(location, [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(third.inbox_id(), first_inbox);
    }

    // verifies: ATCH-040, CONF-030
    #[xmtp_common::test(unwrap_try = true)]
    async fn recorded_data_dir_rejects_mismatched_config_provider() {
        use crate::utils::test::backend::EphemeralBackend;

        let dir = tempfile::tempdir()?;
        let backend = EphemeralBackend::start("").await?;
        let owner = generate_local_wallet();
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let first = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        let original = first.server_configuration().identifier.clone();
        let opened_path = location
            .resolve_identifier(first.inbox_id(), &original)?
            .db_path;
        let db = first.context.store.db();
        let row = db.server_configuration()?.unwrap();
        drop(first);

        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let result = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::edited(|config| {
                    config.identifier = "other-deployment".into();
                }),
            ))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::DeploymentMismatch
            ))
        ));
        assert!(opened_path.is_file());
        assert_eq!(
            location.recorder(backend.url()).unwrap().lookup().await?,
            Some(original.clone())
        );

        db.store_server_configuration(
            "other-deployment",
            &row.backend_url,
            &row.response,
            xmtp_common::time::now_ns(),
        )?;
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let result = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::edited(|config| {
                    config.identifier = original.clone();
                }),
            ))
            .data_location(location, [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::DeploymentMismatch
            ))
        ));
    }

    // verifies: ATCH-040, CONF-030
    #[xmtp_common::test(unwrap_try = true)]
    async fn first_fetch_data_dir_rejects_mismatched_config_provider_before_identity() {
        use crate::utils::test::backend::EphemeralBackend;

        let dir = tempfile::tempdir()?;
        let backend = EphemeralBackend::start("").await?;
        let owner = generate_local_wallet();
        let inbox = identity_setup(&owner).inbox_id().unwrap().to_owned();
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        let mut api_builder = xmtp_api_backend::MessageBackendBuilder::new();
        api_builder.host(backend.url());
        let result = Client::builder(identity_setup(&owner))
            .api_client_with_streams(api_builder.build()?)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::edited(|config| {
                    config.identifier = "other-deployment".into();
                }),
            ))
            .data_location(location.clone(), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::DeploymentMismatch
            ))
        ));

        let fetched = location
            .recorder(backend.url())
            .unwrap()
            .lookup()
            .await?
            .unwrap();
        assert_ne!(fetched, "other-deployment");
        let paths = location.resolve_identifier(&inbox, &fetched)?;
        let mut api = xmtp_api_backend::MockBackendClient::new();
        api.expect_get_configuration().times(0);
        let cached = Client::builder(crate::identity::IdentityStrategy::CachedOnly)
            .api_client(api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::edited(|config| {
                    config.identifier = fetched.clone();
                }),
            ))
            .data_location(
                StorageLocation::Explicit {
                    db_path: paths.db_path,
                    attachments_dir: paths.attachments_dir,
                },
                [0u8; 32].into(),
            )
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await;
        assert!(matches!(
            cached,
            Err(crate::builder::ClientBuilderError::Identity(
                crate::identity::IdentityError::RequiredIdentityNotFound
            ))
        ));
    }

    // verifies: ATCH-040, ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn bound_recorder_keeps_the_opened_deployment() {
        let dir = tempfile::tempdir()?;
        let recorder = DeploymentRecorder::new(dir.path().to_path_buf(), "http://localhost");
        recorder.record("deployment-a").await?;
        let bound = recorder
            .clone()
            .for_opened_identifier("deployment-a".into());
        assert!(matches!(
            bound.record("deployment-b").await,
            Err(StorageLocationError::DeploymentMismatch)
        ));
        assert_eq!(recorder.lookup().await?, Some("deployment-a".into()));
    }

    // verifies: ATCH-077
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn deployment_record_is_readable_under_restrictive_umask() {
        use std::os::unix::fs::PermissionsExt as _;

        struct RestoreUmask(libc::mode_t);
        impl Drop for RestoreUmask {
            fn drop(&mut self) {
                unsafe { libc::umask(self.0) };
            }
        }

        let dir = tempfile::tempdir()?;
        let recorder = DeploymentRecorder::new(dir.path().to_path_buf(), "http://localhost");
        let _umask = RestoreUmask(unsafe { libc::umask(0o400) });
        recorder.record("deployment-a").await?;
        let record_path = dir.path().join("deployments.json");
        assert_eq!(
            std::fs::metadata(&record_path)?.permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(recorder.lookup().await?, Some("deployment-a".into()));
        recorder.record("deployment-b").await?;
        assert_eq!(
            std::fs::metadata(record_path)?.permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(recorder.lookup().await?, Some("deployment-b".into()));
    }

    // Covers plan P19.
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_record_keeps_the_prior_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir()?;
        let recorder = DeploymentRecorder::new(dir.path().to_path_buf(), "http://localhost");
        recorder.record("first").await?;
        let before = tokio::fs::read(dir.path().join("deployments.json")).await?;
        let old_mode = std::fs::metadata(dir.path())?.permissions().mode();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o555))?;
        let result = recorder.record("second").await;
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(old_mode))?;
        assert!(matches!(result, Err(StorageLocationError::Io(_))));
        assert_eq!(
            tokio::fs::read(dir.path().join("deployments.json")).await?,
            before
        );
        let entries = std::fs::read_dir(dir.path())?.collect::<Result<Vec<_>, _>>()?;
        assert_eq!(entries.len(), 1);
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn cancelled_native_deployment_write_removes_temp() {
        use std::sync::Arc;
        let dir = tempfile::tempdir()?;
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        *NATIVE_DEPLOYMENT_WRITE_PAUSE.lock() = Some((entered.clone(), resume.clone()));
        let path = dir.path().to_path_buf();
        let (write, abort) =
            futures::future::abortable(async move { write_file(&path, b"cancelled record").await });
        let task = xmtp_common::task::spawn(write);
        xmtp_common::time::timeout(std::time::Duration::from_secs(3), entered.notified()).await?;
        abort.abort();
        assert!(task.await?.is_err());
        resume.notify_one();
        xmtp_common::time::timeout(std::time::Duration::from_secs(3), async {
            while !dir.path().join("deployments.json").exists() {
                xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await?;
        let temps: Vec<_> = std::fs::read_dir(dir.path())?
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".deployments-")
            })
            .collect();
        assert!(temps.is_empty(), "cancelled write left a temporary file");
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn cancelled_record_write_finishes_before_next_record() {
        use std::sync::Arc;
        let dir = tempfile::tempdir()?;
        let recorder = DeploymentRecorder::new(dir.path().to_path_buf(), "http://localhost");
        let entered = Arc::new(tokio::sync::Notify::new());
        let resume = Arc::new(tokio::sync::Notify::new());
        *NATIVE_DEPLOYMENT_WRITE_PAUSE.lock() = Some((entered.clone(), resume.clone()));
        let first = recorder.clone();
        let (write, abort) = futures::future::abortable(async move { first.record("first").await });
        let first_task = xmtp_common::task::spawn(write);
        xmtp_common::time::timeout(std::time::Duration::from_secs(3), entered.notified()).await?;
        abort.abort();
        assert!(first_task.await?.is_err());

        let second = recorder.clone();
        let (sent, mut received) = tokio::sync::oneshot::channel();
        drop(xmtp_common::task::spawn(async move {
            let _ = sent.send(second.record("second").await);
        }));
        let early =
            tokio::time::timeout(std::time::Duration::from_millis(500), &mut received).await;
        resume.notify_one();
        match early {
            Ok(result) => result??,
            Err(_) => {
                xmtp_common::time::timeout(std::time::Duration::from_secs(3), received).await???
            }
        }
        xmtp_common::time::timeout(std::time::Duration::from_secs(3), async {
            while std::fs::read_dir(dir.path())
                .expect("read deployment directory")
                .filter_map(Result::ok)
                .any(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".deployments-")
                })
            {
                xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await?;
        assert_eq!(recorder.lookup().await?, Some("second".to_owned()));
    }

    // verifies: ATCH-040
    #[xmtp_common::test(unwrap_try = true)]
    async fn data_dir_layout() {
        use crate::utils::test::DefaultTestClientCreator;
        use xmtp_proto::api_client::{ApiBuilder, XmtpBackendClient, XmtpTestClient};
        let dir = tempfile::tempdir()?;
        let owner = generate_local_wallet();
        let inbox = identity_setup(&owner).inbox_id().unwrap().to_string();
        let api = std::sync::Arc::new(DefaultTestClientCreator::create().build()?);
        let backend_url = api.backend_url().unwrap().trim_end_matches('/').to_owned();
        let client = Client::builder(identity_setup(owner))
            .api_client_with_streams(api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(
                StorageLocation::DataDir(dir.path().to_path_buf()),
                [0u8; 32].into(),
            )
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        let identifier = client.server_configuration().identifier.clone();
        let root = dir
            .path()
            .join(deployment_component(&identifier))
            .join(inbox);
        assert!(root.join("xmtp.db3").is_file());
        assert!(root.join("xmtp.db3.sqlcipher_salt").is_file());
        assert!(root.join("attachments").is_dir());
        assert_eq!(
            client.context.attachments.dir.as_deref(),
            Some(root.join("attachments").as_path())
        );
        assert!(dir.path().join("deployments.json").is_file());
        let document: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(dir.path().join("deployments.json")).await?)?;
        assert_eq!(document["deployments"][backend_url.as_str()], identifier);
    }

    // verifies: ATCH-077
    #[cfg(unix)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn fresh_data_dir_uses_private_modes() {
        use std::os::unix::fs::PermissionsExt as _;
        use xmtp_proto::{
            api::mock::MockNetworkClient,
            backend_v1::{
                GetConfigurationResponse, GetInboxIdsRequest, GetInboxIdsResponse,
                get_inbox_ids_response,
            },
        };

        struct UmaskGuard(libc::mode_t);
        impl Drop for UmaskGuard {
            fn drop(&mut self) {
                unsafe { libc::umask(self.0) };
            }
        }

        let dir = tempfile::tempdir()?;
        let data_dir = dir.path().join("new-data-dir");
        let owner = generate_local_wallet();
        let inbox = identity_setup(&owner).inbox_id().unwrap().to_string();
        let configuration_requests = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let observed = configuration_requests.clone();
        let mut mock = MockNetworkClient::new();
        mock.expect_host()
            .return_const("http://config.test".to_owned());
        mock.expect_request().returning(move |_, path, body| {
            let bytes = match path.as_str() {
                "/xmtp.backend.v1.ConfigurationService/GetConfiguration" => {
                    observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    GetConfigurationResponse {
                        identifier: "org.xmtp.test".into(),
                        ..Default::default()
                    }
                    .encode_to_vec()
                }
                "/xmtp.backend.v1.IdentityService/GetInboxIds" => {
                    let request = GetInboxIdsRequest::decode(body)?;
                    GetInboxIdsResponse {
                        responses: request
                            .requests
                            .into_iter()
                            .map(|entry| get_inbox_ids_response::Response {
                                identifier: entry.identifier,
                                identifier_kind: entry.identifier_kind,
                                inbox_id: None,
                            })
                            .collect(),
                    }
                    .encode_to_vec()
                }
                "/xmtp.backend.v1.QueryService/Query" => xmtp_proto::backend_v1::QueryResponse {
                    continuation: Some(Default::default()),
                    ..Default::default()
                }
                .encode_to_vec(),
                other => panic!("unexpected backend request: {other}"),
            };
            Ok(http::Response::new(bytes.into()))
        });
        let _umask = UmaskGuard(unsafe { libc::umask(0) });
        let client = Client::builder(identity_setup(owner))
            .api_client_with_streams(std::sync::Arc::new(xmtp_api_backend::BackendClient::new(
                mock,
            )))
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .data_location(StorageLocation::DataDir(data_dir.clone()), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .with_disable_workers(true)
            .build()
            .await?;
        assert_eq!(
            configuration_requests.load(std::sync::atomic::Ordering::SeqCst),
            1
        );
        let deployment = data_dir.join(deployment_component(
            &client.server_configuration().identifier,
        ));
        let inbox_dir = deployment.join(inbox);
        for path in [
            &data_dir,
            &deployment,
            &inbox_dir,
            &inbox_dir.join("attachments"),
        ] {
            assert_eq!(
                std::fs::metadata(path)?.permissions().mode() & 0o777,
                0o700,
                "{}",
                path.display()
            );
        }
        assert_eq!(
            std::fs::metadata(data_dir.join("deployments.json"))?
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
