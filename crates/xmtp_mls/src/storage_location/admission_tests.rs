//! Admission must finish before a data directory looks up its inbox.

use std::sync::Arc;

use parking_lot::Mutex;

use prost::Message;
use xmtp_configuration::{ServerConfiguration, StaticConfigProvider};
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_id::associations::test_utils::MockSmartContractSignatureVerifier;
use xmtp_proto::{api::mock::MockNetworkClient, backend_v1};

use crate::{
    Client, InboxOwner, builder::ClientBuilderError, client::ClientError,
    identity::IdentityStrategy, storage_location::StorageLocation, utils::VersionInfo,
};

async fn rejected_directory_build(
    mut response: backend_v1::GetConfigurationResponse,
    use_provider: bool,
) -> Result<ClientBuilderError, xmtp_common::BoxDynError> {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let observed = requests.clone();
    let configuration = ServerConfiguration::from(response.clone());
    if use_provider {
        // Only the provider refuses these builds. The fetched deployment
        // identifier must still match before its snapshot can be used.
        response.auth = None;
        response.min_libxmtp_version.clear();
    }
    let mut network = MockNetworkClient::new();
    network
        .expect_host()
        .return_const("http://admission.test".to_owned());
    network.expect_request().returning(move |_, path, body| {
        observed.lock().push(path.as_str().to_owned());
        let bytes = match path.as_str() {
            "/xmtp.backend.v1.ConfigurationService/GetConfiguration" => response.encode_to_vec(),
            "/xmtp.backend.v1.IdentityService/GetInboxIds" => {
                let request = backend_v1::GetInboxIdsRequest::decode(body)?;
                backend_v1::GetInboxIdsResponse {
                    responses: request
                        .requests
                        .into_iter()
                        .map(|entry| backend_v1::get_inbox_ids_response::Response {
                            identifier: entry.identifier,
                            identifier_kind: entry.identifier_kind,
                            inbox_id: None,
                        })
                        .collect(),
                }
                .encode_to_vec()
            }
            other => panic!("unexpected backend request: {other}"),
        };
        Ok(http::Response::new(bytes.into()))
    });
    let dir = tempfile::tempdir()?;
    let mut version = VersionInfo::default();
    version.test_update_version("1.2.3");
    let mut builder = Client::builder(IdentityStrategy::for_identifier(
        generate_local_wallet().get_identifier()?,
        1,
    ))
    .api_client_with_streams(Arc::new(xmtp_api_backend::BackendClient::new(network)))
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .version(version)
    .with_disable_workers(true);
    if use_provider {
        builder = builder.config_provider(Arc::new(StaticConfigProvider::new(configuration)));
    }
    let error = builder
        .data_location(
            StorageLocation::DataDir(dir.path().to_path_buf()),
            [0u8; 32].into(),
        )
        .await?
        .default_mls_store()?
        .build()
        .await
        .err()
        .expect("the deployment must refuse the build");
    assert_eq!(
        requests.lock().as_slice(),
        ["/xmtp.backend.v1.ConfigurationService/GetConfiguration"],
        "admission must refuse the build before any inbox request"
    );
    assert!(std::fs::read_dir(dir.path())?.next().is_none());
    Ok(error)
}

// verifies: CONF-026, CONF-051, CONF-064
#[xmtp_common::test(unwrap_try = true)]
async fn directory_admission_requires_credentials_before_inbox_lookup() {
    for use_provider in [false, true] {
        let error = rejected_directory_build(
            backend_v1::GetConfigurationResponse {
                identifier: "org.xmtp.admission".into(),
                auth: Some(backend_v1::AuthConfiguration {
                    enabled: true,
                    required_scopes: vec!["review:send".into()],
                    ..Default::default()
                }),
                ..Default::default()
            },
            use_provider,
        )
        .await?;
        assert!(matches!(
            error,
            ClientBuilderError::ClientError(ClientError::AuthRequired { required_scopes })
                if required_scopes == ["review:send"]
        ));
    }
}

// verifies: CONF-026, CONF-049, CONF-064
#[xmtp_common::test(unwrap_try = true)]
async fn directory_admission_checks_minimum_version_before_inbox_lookup() {
    for use_provider in [false, true] {
        let error = rejected_directory_build(
            backend_v1::GetConfigurationResponse {
                identifier: "org.xmtp.admission".into(),
                min_libxmtp_version: "2.0.0".into(),
                ..Default::default()
            },
            use_provider,
        )
        .await?;
        assert!(matches!(
            error,
            ClientBuilderError::ClientError(ClientError::ClientVersionTooOld { client, minimum })
                if client == "1.2.3" && minimum == "2.0.0"
        ));
    }
}
