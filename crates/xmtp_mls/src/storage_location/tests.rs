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

    // verifies: P19
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

    // verifies: ATCH-069, CONF-026
    #[xmtp_common::test(unwrap_try = true)]
    async fn build_records_the_stored_identifier() {
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
            .await?;
        assert_eq!(second.server_configuration().identifier, "new-deployment");
        assert_eq!(
            location.recorder(backend.url()).unwrap().lookup().await?,
            Some("new-deployment".to_owned())
        );
    }

    // verifies: P19
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
}
