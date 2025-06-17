use crate::utils::test::TestClient as TestApiClient;
use crate::utils::TestXmtpMlsContext;
use crate::{client::Client, configuration::DeviceSyncUrls, identity::IdentityStrategy};
use alloy::signers::local::PrivateKeySigner;
use xmtp_id::associations::test_utils::WalletTestExt;
use xmtp_id::{associations::builder::SignatureRequest, InboxOwner};
use xmtp_proto::api_client::{ApiBuilder, XmtpTestClient};

pub type BenchClient = Client<TestXmtpMlsContext>;

/// Create a new, yet-unregistered client
pub async fn new_unregistered_client(history_sync: bool) -> (BenchClient, PrivateKeySigner) {
    let _ = fdlimit::raise_fd_limit();

    let nonce = 1;
    let wallet = xmtp_cryptography::utils::generate_local_wallet();
    let ident = wallet.identifier();
    let inbox_id = ident.inbox_id(nonce).unwrap();

    let dev = std::env::var("DEV_GRPC");
    let is_dev_network = matches!(dev, Ok(d) if d == "true" || d == "1");

    let api_client = if is_dev_network {
        tracing::info!("Using Dev GRPC");
        <TestApiClient as XmtpTestClient>::create_dev()
            .build()
            .await
            .unwrap()
    } else {
        tracing::info!("Using Local GRPC");
        <TestApiClient as XmtpTestClient>::create_local()
            .build()
            .await
            .unwrap()
    };

    let client = crate::Client::builder(IdentityStrategy::new(
        inbox_id,
        wallet.identifier(),
        nonce,
        None,
    ));

    let mut client = client
        .temp_store()
        .await
        .api_client(api_client)
        .with_remote_verifier()
        .unwrap()
        .default_mls_store()
        .unwrap();

    if history_sync {
        client = client.device_sync_server_url(DeviceSyncUrls::LOCAL_ADDRESS);
    }
    let client = client.build().await.unwrap();

    (client, wallet)
}

/// Add ECDSA Signature to a client
pub async fn ecdsa_signature(client: &BenchClient, owner: impl InboxOwner) -> SignatureRequest {
    let mut signature_request = client.context.signature_request().unwrap();
    let signature_text = signature_request.signature_text();
    let unverified_signature = owner.sign(&signature_text).unwrap();
    signature_request
        .add_signature(unverified_signature, client.scw_verifier())
        .await
        .unwrap();

    signature_request
}

/// Create a new registered client with an EOA
pub async fn new_client(history_sync: bool) -> BenchClient {
    let (client, wallet) = new_unregistered_client(history_sync).await;
    let signature_request = ecdsa_signature(&client, wallet).await;
    client.register_identity(signature_request).await.unwrap();
    client
}
