//! Storage location tests: inbox lookup.

use super::*;

// The request that looks up an inbox carries the identifier.
// verifies: CONF-026
#[xmtp_common::test(unwrap_try = true)]
async fn data_dir_looks_up_an_inbox_only_after_the_configuration() {
    let dir = tempfile::tempdir()?;
    let url = "http://127.0.0.1:1";
    seed_deployment(dir.path(), url, "recorded-deployment").await?;
    let identifier = generate_local_wallet().get_identifier()?;
    let api = || {
        let mut api = xmtp_api_backend::MessageBackendBuilder::new();
        api.host(url);
        api.build()
    };
    let build = |api, allow_offline| {
        let identifier = identifier.clone();
        let dir = dir.path().to_path_buf();
        async move {
            Client::builder(crate::identity::IdentityStrategy::for_identifier(
                identifier, 1,
            ))
            .api_client_with_streams(api)
            .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
            .with_allow_offline(Some(allow_offline))
            .data_location(StorageLocation::DataDir(dir), [0u8; 32].into())
            .await?
            .default_mls_store()?
            .build()
            .await
        }
    };
    let offline = build(api()?, true).await;
    assert!(
        matches!(
            offline,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::InboxId
            ))
        ),
        "offline: {:?}",
        offline.err()
    );
    // The backend is unreachable, so the first request fails the build.
    let online = build(api()?, false).await;
    assert!(
        matches!(
            online,
            Err(crate::builder::ClientBuilderError::ClientError(
                crate::client::ClientError::ConfigurationUnavailable(_)
            ))
        ),
        "online: {:?}",
        online.err()
    );
    assert!(
        !dir.path()
            .join(deployment_component("recorded-deployment"))
            .exists()
    );
}

// verifies: CONF-030
#[xmtp_common::test(unwrap_try = true)]
async fn data_dir_inbox_lookup_refuses_a_deployment_other_than_the_record() {
    use crate::utils::test::backend::EphemeralBackend;
    let dir = tempfile::tempdir()?;
    let backend = EphemeralBackend::start("").await?;
    seed_deployment(dir.path(), backend.url(), "recorded-deployment").await?;
    let mut api = xmtp_api_backend::MessageBackendBuilder::new();
    api.host(backend.url());
    let result = Client::builder(crate::identity::IdentityStrategy::for_identifier(
        generate_local_wallet().get_identifier()?,
        1,
    ))
    .api_client_with_streams(api.build()?)
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .data_location(
        StorageLocation::DataDir(dir.path().to_path_buf()),
        [0u8; 32].into(),
    )
    .await?
    .default_mls_store()?
    .build()
    .await;

    assert!(
        matches!(
            &result,
            Err(crate::builder::ClientBuilderError::ClientError(
                crate::client::ClientError::BackendMismatch { stored, received }
            )) if stored == "recorded-deployment" && received != stored
        ),
        "build: {:?}",
        result.err()
    );
    let entries: Vec<_> = std::fs::read_dir(dir.path())?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<_, _>>()?;
    assert_eq!(entries, ["deployments.json"]);
}

// verifies: CONF-030
#[xmtp_common::test(unwrap_try = true)]
async fn data_dir_inbox_lookup_keeps_the_stored_configuration() {
    use crate::utils::test::backend::EphemeralBackend;
    let dir = tempfile::tempdir()?;
    let backend = EphemeralBackend::start("").await?;
    let owner = generate_local_wallet();
    let location = StorageLocation::DataDir(dir.path().to_path_buf());
    let mut api = xmtp_api_backend::MessageBackendBuilder::new();
    api.host(backend.url());
    let first = Client::builder(identity_setup(&owner))
        .api_client_with_streams(api.build()?)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .data_location(location.clone(), [0u8; 32].into())
        .await?
        .default_mls_store()?
        .with_disable_workers(true)
        .build()
        .await?;
    // The inbox's database is bound to another deployment.
    let db = first.context.db();
    let row = db.server_configuration()?.unwrap();
    let mut response =
        xmtp_proto::backend_v1::GetConfigurationResponse::decode(row.response.as_slice())?;
    response.identifier = "stored-deployment".to_owned();
    db.store_server_configuration(
        &response.identifier,
        &row.backend_url,
        &response.encode_to_vec(),
        row.fetched_at_ns,
    )?;
    drop(first);

    let mut api = xmtp_api_backend::MessageBackendBuilder::new();
    api.host(backend.url());
    let second = Client::builder(crate::identity::IdentityStrategy::for_identifier(
        owner.get_identifier()?,
        1,
    ))
    .api_client_with_streams(api.build()?)
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .data_location(location, [0u8; 32].into())
    .await?
    .default_mls_store()?
    .with_disable_workers(true)
    .build()
    .await;

    assert!(
        matches!(
            second,
            Err(crate::builder::ClientBuilderError::StorageLocation(
                StorageLocationError::DeploymentMismatch
            ))
        ),
        "build: {:?}",
        second.err()
    );
    assert_eq!(
        db.server_configuration()?.unwrap().identifier,
        "stored-deployment"
    );
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
