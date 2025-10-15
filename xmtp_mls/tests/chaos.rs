use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::{ConnectionError, XmtpTestDb};
use xmtp_db_test::ChaosDb;
use xmtp_id::InboxOwner;
use xmtp_mls::{
    Client,
    identity::IdentityStrategy,
    utils::test::{TestClient, register_client},
};
use xmtp_proto::api_client::ApiBuilder;
use xmtp_proto::api_client::XmtpTestClient;

fn new_identity(owner: impl InboxOwner) -> IdentityStrategy {
    let nonce = 1;
    let ident = owner.get_identifier().unwrap();
    let inbox_id = ident.inbox_id(nonce).unwrap();
    IdentityStrategy::new(inbox_id, ident, nonce, None)
}

#[xmtp_common::test]
#[should_panic]
async fn chaos_demo() {
    let owner = generate_local_wallet();
    let store = xmtp_db::DefaultStore::create_persistent_store(None).await;
    let (chaos, store) = ChaosDb::builder(store).error_frequency(0.0).build();
    let alix = Client::builder(new_identity(&owner))
        .store(store)
        .api_clients(
            TestClient::create_local().build().unwrap(),
            TestClient::create_local().build().unwrap(),
        )
        .default_mls_store()
        .unwrap()
        .with_remote_verifier()
        .unwrap()
        .build()
        .await
        .unwrap();
    register_client(&alix, &owner).await;

    // return an error on next database access
    chaos.post_read_hook(|_c| Err(ConnectionError::Database(diesel::result::Error::NotFound)));

    alix.find_groups(Default::default()).unwrap();
}
