//! Tests for network connectivity, offline behavior, and API statistics

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn radio_silence() {
    let alex = TesterBuilder::new()
        .sync_worker()
        .sync_server()
        .stream()
        .build()
        .await;

    let convo_callback = Arc::new(RustStreamCallback::default());
    let _convo_stream_handle = alex.conversations().stream_groups(convo_callback).await;

    let worker = alex.client.inner_client.context.sync_metrics().unwrap();

    let stats = alex.inner_client.api_stats();
    let ident_stats = alex.inner_client.identity_api_stats();

    // One identity update pushed. Zero interaction with groups.
    assert_eq!(ident_stats.publish_identity_update.get_count(), 1);
    assert_eq!(stats.send_welcome_messages.get_count(), 0);
    assert_eq!(stats.send_group_messages.get_count(), 2);

    let bo = Tester::new().await;
    let conversation = alex
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    conversation
        .send(b"Hello there".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    worker
        .register_interest(SyncMetric::ConsentSent, 1)
        .wait()
        .await
        .unwrap();

    // One identity update pushed. Zero interaction with groups.
    assert_eq!(ident_stats.publish_identity_update.get_count(), 1);
    assert_eq!(ident_stats.get_inbox_ids.get_count(), 2);
    assert_eq!(stats.send_welcome_messages.get_count(), 1);
    let group_message_count = stats.send_group_messages.get_count();

    // Sleep for a bit and make sure nothing else has sent
    tokio::time::sleep(Duration::from_secs(5)).await;

    // One identity update pushed. Zero interaction with groups.
    assert_eq!(ident_stats.publish_identity_update.get_count(), 1);
    assert_eq!(ident_stats.get_inbox_ids.get_count(), 2);
    assert_eq!(stats.send_welcome_messages.get_count(), 1);
    assert_eq!(stats.send_group_messages.get_count(), group_message_count);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn create_client_does_not_hit_network() {
    let ffi_inbox_owner = FfiWalletInboxOwner::new();
    let nonce = 1;
    let ident = ffi_inbox_owner.identifier();
    let inbox_id = ident.inbox_id(nonce).unwrap();
    let path = tmp_path();
    let key = static_enc_key().to_vec();

    let connection = connect_to_backend_test().await;
    let client = create_client(
        connection.clone(),
        connect_to_backend_test().await,
        Some(path.clone()),
        Some(key.clone()),
        &inbox_id,
        ffi_inbox_owner.identifier(),
        nonce,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let signature_request = client.signature_request().unwrap().clone();
    register_client_with_wallet(&ffi_inbox_owner, &client).await;

    signature_request
        .add_wallet_signature(&ffi_inbox_owner.wallet)
        .await;

    tokio::time::sleep(Duration::from_secs(2)).await;

    let aggregate_str = client.api_aggregate_statistics();
    println!("Aggregate Stats Create:\n{}", aggregate_str);

    let api_stats = client.api_statistics();
    assert_eq!(api_stats.upload_key_package, 1);
    assert_eq!(api_stats.fetch_key_package, 0);

    let identity_stats = client.api_identity_statistics();
    assert_eq!(identity_stats.publish_identity_update, 1);
    assert_eq!(identity_stats.get_identity_updates_v2, 2);
    assert_eq!(identity_stats.get_inbox_ids, 1);
    assert_eq!(identity_stats.verify_smart_contract_wallet_signature, 0);

    client.clear_all_statistics();

    let build = create_client(
        connection.clone(),
        connect_to_backend_test().await,
        Some(path.clone()),
        Some(key.clone()),
        &inbox_id,
        ffi_inbox_owner.identifier(),
        nonce,
        None,
        None,
        None,
        Some(true),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_secs(2)).await;

    let aggregate_str = build.api_aggregate_statistics();
    println!("Aggregate Stats Build:\n{}", aggregate_str);

    let api_stats = build.api_statistics();
    assert_eq!(api_stats.upload_key_package, 0);
    assert_eq!(api_stats.fetch_key_package, 0);

    let identity_stats = build.api_identity_statistics();
    assert_eq!(identity_stats.publish_identity_update, 0);
    assert_eq!(identity_stats.get_identity_updates_v2, 0);
    assert_eq!(identity_stats.get_inbox_ids, 0);
    assert_eq!(identity_stats.verify_smart_contract_wallet_signature, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn ffi_api_stats_exposed_correctly() {
    let tester = Tester::new().await;
    let client: &FfiXmtpClient = &tester.client;

    let bo = Tester::new().await;
    let _conversation = client
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let _ = client
        .conversations()
        .list(FfiListConversationsOptions::default());

    let api_stats = client.api_statistics();
    tracing::info!(
        "api_stats.send_group_messages {}",
        api_stats.send_group_messages
    );
    assert!(api_stats.send_group_messages == 1);
    assert!(api_stats.send_welcome_messages == 1);

    let identity_stats = client.api_identity_statistics();
    assert_eq!(identity_stats.publish_identity_update, 1);
    assert!(identity_stats.get_inbox_ids >= 1);

    let aggregate_str = client.api_aggregate_statistics();
    println!("Aggregate Stats:\n{}", aggregate_str);

    assert!(aggregate_str.contains("UploadKeyPackage"));
    assert!(aggregate_str.contains("PublishIdentityUpdate"));

    client.clear_all_statistics();

    let api_stats = client.api_statistics();
    assert!(api_stats.send_group_messages == 0);
    assert!(api_stats.send_welcome_messages == 0);

    let identity_stats = client.api_identity_statistics();
    assert_eq!(identity_stats.publish_identity_update, 0);
    assert!(identity_stats.get_inbox_ids == 0);

    let aggregate_str = client.api_aggregate_statistics();
    println!("Aggregate Stats:\n{}", aggregate_str);

    let _conversation2 = client
        .conversations()
        .create_group(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let api_stats = client.api_statistics();
    assert!(api_stats.send_group_messages == 1);
    assert!(api_stats.send_welcome_messages == 1);

    let identity_stats = client.api_identity_statistics();
    assert_eq!(identity_stats.publish_identity_update, 0);
    assert!(identity_stats.get_inbox_ids == 1);

    let aggregate_str = client.api_aggregate_statistics();
    println!("Aggregate Stats:\n{}", aggregate_str);
}

#[tokio::test]
async fn test_is_connected_after_connect() {
    let api_backend = connect_to_backend_test().await;

    let connected = is_connected(api_backend).await;

    assert!(connected, "Expected API client to report as connected");

    let api = connect_to_backend(
        "http://127.0.0.1:59999".to_string(),
        None,
        false,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let backend = MessageBackendBuilder::default()
        .from_bundle(api.0.clone())
        .unwrap();
    let api = ApiClientWrapper::new(backend, Default::default());
    let result = api
        .query_group_messages(xmtp_common::rand_vec::<16>().into())
        .await;
    assert!(result.is_err(), "Expected connection to fail");
}
