//! Storage location tests: location validation.

use super::*;

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
        client
            .context
            .features()
            .attachments
            .dir()
            .map(|dir| dir.as_path()),
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
