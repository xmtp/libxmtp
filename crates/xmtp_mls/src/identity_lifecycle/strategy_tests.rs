//! How an identity strategy reports the identity it initializes.

#[xmtp_common::test(unwrap_try = true)]
async fn initialize_identity_debug_log_reports_strategy() {
    use crate::context::test_support::identity_of;
    use crate::identity::IdentityStrategy;
    use tracing::instrument::WithSubscriber;
    use xmtp_api::ApiClientWrapper;
    use xmtp_api_backend::MockBackendClient;
    use xmtp_common::Retry;
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_db::{Store, XmtpTestDb, identity::StoredIdentity, sql_key_store::SqlKeyStore};
    use xmtp_id::associations::test_utils::{MockSmartContractSignatureVerifier, WalletTestExt};
    use xmtp_logging::{Level, test_logging::LogCapture};

    let store = xmtp_db::TestDb::create_persistent_store(None).await;
    let identifier = generate_local_wallet().identifier();
    let inbox_id = identifier.inbox_id(0).unwrap();
    let stored: StoredIdentity = (&identity_of(&inbox_id)).try_into().unwrap();
    stored.store(&store.conn()).unwrap();

    let strategy = IdentityStrategy::new(inbox_id.clone(), identifier, 0);
    let api = ApiClientWrapper::new(MockBackendClient::new(), Retry::default());
    let verifier = MockSmartContractSignatureVerifier::new(true);
    let capture = LogCapture::new(Level::Debug);
    let loaded = strategy
        .initialize_identity(&api, &SqlKeyStore::new(&store.db()), &verifier)
        .with_subscriber(capture.dispatch())
        .await
        .unwrap();

    let output = capture.output();
    assert_eq!(loaded.inbox_id(), inbox_id);
    assert!(output.contains("identity strategy"), "{output}");
    assert!(
        output.contains("\"strategy\":\"create_if_not_found\""),
        "{output}"
    );
    assert!(
        output.contains("\"stored_identity_present\":true"),
        "{output}"
    );
}
