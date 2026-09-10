//! Tests for network connectivity, offline behavior, and API statistics

use super::*;
use xmtp_mls::context::XmtpSharedContext;
use xmtp_proto::api::HasStats;

#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 1)]
async fn radio_silence() {
    let alex = TesterBuilder::new().sync_worker().stream().build().await;

    let convo_callback = Arc::new(RustStreamCallback::default());
    let _convo_stream_handle = alex.conversations().stream_groups(convo_callback).await;

    let worker = alex.client.inner_client.context.sync_metrics().unwrap();

    let stats = alex.inner_client.context.api().api_client.mls_stats();
    let ident_stats = alex.inner_client.context.api().api_client.identity_stats();

    // Publish the key package, identity, and sync group.
    assert_eq!(stats.publish.get_count(), 3);

    let bo = Tester::new().await;
    let conversation = alex
        .conversations()
        .create_group_by_identity(
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

    assert_eq!(ident_stats.get_inbox_ids.get_count(), 2);
    let publish_count = stats.publish.get_count();

    // Sleep for a bit and make sure nothing else has sent
    tokio::time::sleep(Duration::from_secs(5)).await;

    assert_eq!(ident_stats.get_inbox_ids.get_count(), 2);
    assert_eq!(stats.publish.get_count(), publish_count);
}

#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 1)]
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
        DbOptions::new(Some(path.clone()), Some(key.clone()), None, None, None),
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
    // The sync worker also publishes its group.
    assert_eq!(api_stats.publish, 3);
    assert_eq!(api_stats.query_newest, 0);

    let identity_stats = client.api_identity_statistics();
    assert_eq!(api_stats.query, 5);
    assert_eq!(identity_stats.get_inbox_ids, 1);
    assert_eq!(identity_stats.verify_smart_contract_wallet_signatures, 0);

    client.clear_all_statistics();

    let build = create_client(
        connection.clone(),
        DbOptions::new(Some(path.clone()), Some(key.clone()), None, None, None),
        &inbox_id,
        ffi_inbox_owner.identifier(),
        nonce,
        None,
        None,
        Some(true),
        None,
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
    assert_eq!(api_stats.publish, 0);
    assert_eq!(api_stats.query_newest, 0);

    let identity_stats = build.api_identity_statistics();
    assert_eq!(api_stats.query, 0);
    assert_eq!(identity_stats.get_inbox_ids, 0);
    assert_eq!(identity_stats.verify_smart_contract_wallet_signatures, 0);
}

#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 1)]
async fn ffi_api_stats_exposed_correctly() {
    let tester = Tester::new().await;
    let client: &FfiXmtpClient = &tester.client;

    let bo = Tester::new().await;
    let _conversation = client
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let _ = client
        .conversations()
        .list(FfiListConversationsOptions::default());

    let api_stats = client.api_statistics();
    assert_eq!(api_stats.publish, 4);
    let identity_stats = client.api_identity_statistics();
    assert!(identity_stats.get_inbox_ids >= 1);

    let aggregate_str = client.api_aggregate_statistics();
    println!("Aggregate Stats:\n{}", aggregate_str);

    assert!(aggregate_str.contains("publish"));
    assert!(aggregate_str.contains("get_inbox_ids"));

    client.clear_all_statistics();

    let api_stats = client.api_statistics();
    assert_eq!(api_stats.publish, 0);

    let identity_stats = client.api_identity_statistics();
    assert!(identity_stats.get_inbox_ids == 0);

    let aggregate_str = client.api_aggregate_statistics();
    println!("Aggregate Stats:\n{}", aggregate_str);

    let _conversation2 = client
        .conversations()
        .create_group_by_identity(
            vec![bo.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();

    let api_stats = client.api_statistics();
    assert_eq!(api_stats.publish, 2);

    let identity_stats = client.api_identity_statistics();
    assert!(identity_stats.get_inbox_ids == 1);

    let aggregate_str = client.api_aggregate_statistics();
    println!("Aggregate Stats:\n{}", aggregate_str);
}

#[xmtp_common::test(unwrap_try = true)]
async fn test_is_connected_after_connect() {
    let api_backend = connect_to_backend_test().await;

    let connected = is_connected(api_backend).await;

    assert!(connected, "Expected API client to report as connected");

    let api = connect_to_backend("http://127.0.0.1:59999".to_string(), None, None, None, None)
        .await
        .unwrap();
    let result = api
        .wrapper
        .query_group_messages(xmtp_common::rand_array::<16>().into())
        .await;
    assert!(result.is_err(), "Expected connection to fail");
}

#[xmtp_common::test(unwrap_try = true)]
async fn backend_url_is_required() {
    let result = connect_to_backend(String::new(), None, None, None, None).await;
    assert!(result.is_err());
}

#[xmtp_common::test(unwrap_try = true)]
async fn api_client_cache_key_uses_backend_url_and_app_version() {
    let url = std::env::var("XMTP_BACKEND_URL")
        .unwrap_or_else(|_| xmtp_configuration::BACKEND_TEST_URL.into());
    let client =
        connect_to_backend(url.clone(), None, Some("TestApp/1.0".into()), None, None).await?;
    assert_eq!(client.cache_key(), format!("{url}|TestApp/1.0"));
    let default = connect_to_backend(url.clone(), None, None, None, None).await?;
    assert_eq!(default.cache_key(), format!("{url}|"));
    assert_ne!(default.cache_key(), client.cache_key());
    let other = connect_to_backend("http://127.0.0.1:59999".into(), None, None, None, None).await?;
    assert_ne!(default.cache_key(), other.cache_key());
}
