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

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use crate::{Client, InboxOwner, utils::test::identity_setup};
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;

    fn builder() -> crate::builder::ClientBuilder<xmtp_api_backend::MockBackendClient, ()> {
        let owner = generate_local_wallet();
        let api = xmtp_api_backend::MockBackendClient::new();
        Client::builder(identity_setup(owner))
            .api_client(api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    }

    fn response(identifier: &str) -> xmtp_proto::backend_v1::GetConfigurationResponse {
        xmtp_proto::backend_v1::GetConfigurationResponse {
            identifier: identifier.to_owned(),
            ..Default::default()
        }
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn offline_uses_record() {
        let dir = tempfile::tempdir()?;
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        let recorder = location.recorder("").unwrap();
        recorder.record("acme/prod").await?;
        let mut builder = builder();
        builder
            .api_client
            .as_mut()
            .unwrap()
            .expect_get_configuration()
            .times(0);
        let builder = builder
            .with_allow_offline(Some(true))
            .data_location(location, [0u8; 32].into())
            .await?;
        assert!(
            builder
                .attachments_dir
                .unwrap()
                .to_string_lossy()
                .contains(&deployment_component("acme/prod"))
        );

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
        let dir = tempfile::tempdir()?;
        let mut builder = builder();
        builder
            .api_client
            .as_mut()
            .unwrap()
            .expect_get_configuration()
            .times(1)
            .returning(|_| Ok(response("acme/prod")));
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        let resolved = builder
            .data_location(location.clone(), [0u8; 32].into())
            .await?;
        assert!(
            resolved
                .attachments_dir
                .unwrap()
                .to_string_lossy()
                .contains(&deployment_component("acme/prod"))
        );
        assert_eq!(
            location.recorder("").unwrap().lookup().await?,
            Some("acme/prod".to_owned())
        );
        let document: serde_json::Value =
            serde_json::from_slice(&tokio::fs::read(dir.path().join("deployments.json")).await?)?;
        assert_eq!(document["version"], 1);
        assert_eq!(document["deployments"][""], "acme/prod");
    }

    // verifies: P19
    #[xmtp_common::test(unwrap_try = true)]
    async fn torn_file_treated_empty() {
        let dir = tempfile::tempdir()?;
        tokio::fs::write(
            dir.path().join("deployments.json"),
            b"{\"version\":1,\"deployments\":{",
        )
        .await?;
        let mut builder = builder();
        builder
            .api_client
            .as_mut()
            .unwrap()
            .expect_get_configuration()
            .times(1)
            .returning(|_| Ok(response("after-torn-write")));
        let location = StorageLocation::DataDir(dir.path().to_path_buf());
        builder
            .data_location(location.clone(), [0u8; 32].into())
            .await?;
        assert_eq!(
            location.recorder("").unwrap().lookup().await?,
            Some("after-torn-write".to_owned())
        );
    }

    // verifies: ATCH-069
    #[xmtp_common::test(unwrap_try = true)]
    async fn offline_first_start_without_record_fails() {
        let dir = tempfile::tempdir()?;
        let mut builder = builder();
        builder
            .api_client
            .as_mut()
            .unwrap()
            .expect_get_configuration()
            .times(0);
        let result = builder
            .with_allow_offline(Some(true))
            .data_location(
                StorageLocation::DataDir(dir.path().to_path_buf()),
                [0u8; 32].into(),
            )
            .await;
        assert!(matches!(
            result,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::OfflineMissingDeployment
            ))
        ));
        assert!(!dir.path().join("deployments.json").exists());
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
