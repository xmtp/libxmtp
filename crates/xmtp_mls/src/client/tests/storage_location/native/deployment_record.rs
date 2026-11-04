//! Storage location tests: deployment record.

use super::*;

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
            .features()
            .attachments
            .dir()
            .unwrap()
            .to_string_lossy()
            .contains(&deployment_component(&identifier))
    );
    assert_eq!(
        recorded_deployment(&location, backend.url()).await?,
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
    assert_eq!(
        client.context.features().attachments.dir().cloned(),
        Some(paths.attachments_dir)
    );
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
        recorded_deployment(&location, backend.url()).await?,
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
        recorded_deployment(
            &StorageLocation::DataDir(dir.path().to_path_buf()),
            backend.url()
        )
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
    let db = first.context.db();
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
        recorded_deployment(&location, backend.url()).await?,
        Some(original_identifier.clone())
    );
    let wrong = dir
        .path()
        .join(deployment_component("new-deployment"))
        .join(first_inbox.to_ascii_lowercase())
        .join("xmtp.db3");
    assert!(!wrong.exists());

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
        .resolve(backend.url(), first.inbox_id())
        .await?
        .db_path;
    let db = first.context.db();
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
        recorded_deployment(&location, backend.url()).await?,
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

    let fetched = recorded_deployment(&location, backend.url())
        .await?
        .unwrap();
    assert_ne!(fetched, "other-deployment");
    let paths = location.resolve(backend.url(), &inbox).await?;
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
