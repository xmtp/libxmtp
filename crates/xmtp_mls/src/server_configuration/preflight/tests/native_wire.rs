use super::*;
use futures::StreamExt;
use xmtp_proto::{
    api_client::XmtpMlsBidiStreams,
    types::{Cursor, IncomingBatchLimits, Topic},
};
const LIMITS: IncomingBatchLimits = IncomingBatchLimits {
    max_rows: 8,
    max_bytes: 1024,
};

// verifies: CONF-077
#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_stays_per_client_on_an_existing_native_wire() {
    let (moved, script) = fixture_options(true, false).await;
    let raw = moved.context.api().api_client.raw_for_test().clone();
    let a_store = TestDb::create_ephemeral_store().await;
    a_store.db().store_server_configuration(
        IDENTIFIER,
        NEW,
        &response(IDENTIFIER).encode_to_vec(),
        1,
    )?;
    let active = Client::builder(IdentityStrategy::ExternalIdentity(
        crate::identity::Identity::mock_identity(),
    ))
    .store(a_store)
    .api_client_with_streams(raw)
    .with_allow_offline(Some(true))
    .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
    .with_disable_workers(true)
    .default_mls_store()?
    .build()
    .await?;
    let a_factory = active.context.incoming_runtime().factory.as_ref().unwrap();
    let b_factory = moved.context.incoming_runtime().factory.as_ref().unwrap();
    let a_topic = Topic::new_group_message([1; 16]);
    let b_topic = Topic::new_group_message([2; 16]);
    let later_a_topic = Topic::new_group_message([3; 16]);
    let a_lease = a_factory
        .open([(a_topic.clone(), Cursor(0))].into(), LIMITS)
        .await?;
    let mut wire = script.wire.lock().take().expect("one shared wire opened");
    let sent = wire.next().await.unwrap();
    assert!(
        matches!(sent.request, Some(wire::subscribe_request::Request::Update(update)) if update.adds.len() == 1)
    );
    assert_eq!(*script.calls.lock(), vec!["bidi"]);

    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    script
        .responses
        .lock()
        .push_back(Ok(response("org.example.other")));
    let mut b_open = b_factory.open([(b_topic, Cursor(0))].into(), LIMITS);
    assert!(futures::poll!(&mut b_open).is_pending());
    assert!(
        futures::poll!(wire.next()).is_pending(),
        "moved client sent interest before validation"
    );
    send.send(()).unwrap();
    let error = b_open.await.err().expect("mismatch must refuse B");
    assert!(matches!(cause(&error), ClientError::BackendMismatch { .. }));
    assert!(
        futures::poll!(wire.next()).is_pending(),
        "rejected client sent interest"
    );

    let later_a = a_factory
        .open([(later_a_topic, Cursor(0))].into(), LIMITS)
        .await?;
    let sent = wire.next().await.unwrap();
    assert!(
        matches!(sent.request, Some(wire::subscribe_request::Request::Update(update)) if update.adds.len() == 1)
    );
    assert_eq!(
        script
            .calls
            .lock()
            .iter()
            .filter(|&&call| call == "bidi")
            .count(),
        1,
        "A retained the existing wire"
    );
    drop((a_lease, later_a, wire));
    active.close().await?;
    moved.close().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_guards_direct_native_bidi_open() {
    let (client, script) = fixture().await;
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let open = client
        .context
        .api()
        .api_client
        .subscribe_bidi(Box::pin(futures::stream::empty()));
    futures::pin_mut!(open);
    assert!(futures::poll!(&mut open).is_pending());
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    send.send(()).unwrap();
    drop(open.await?);
    assert_eq!(*script.calls.lock(), vec!["configuration", "bidi"]);
    client.close().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_default_remote_verifier_cannot_use_an_unguarded_api() {
    let (client, script) = fixture_options(false, true).await;
    let verifier = client.scw_verifier();
    let (send, receive) = oneshot::channel();
    *script.pause.lock() = Some(receive);
    let account = xmtp_id::associations::AccountId::new_evm(
        1,
        "0x0000000000000000000000000000000000000001".to_owned(),
    );
    let verify = verifier.is_valid_signature(account, [0; 32], Vec::new().into(), None);
    futures::pin_mut!(verify);
    assert!(futures::poll!(&mut verify).is_pending());
    assert_eq!(*script.calls.lock(), vec!["configuration"]);
    send.send(()).unwrap();
    // The fake backend returns no SCW response; only dispatch ordering is at issue.
    assert!(verify.await.is_err());
    assert_eq!(
        *script.calls.lock(),
        vec!["configuration", "verify_smart_contract_wallet_signatures"]
    );
    client.close().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_does_not_replace_an_app_supplied_verifier() {
    let (client, script) = fixture().await;
    let verifier = client.scw_verifier();
    let account = xmtp_id::associations::AccountId::new_evm(
        1,
        "0x0000000000000000000000000000000000000001".to_owned(),
    );
    let result = verifier
        .is_valid_signature(account, [0; 32], Vec::new().into(), None)
        .await?;
    assert!(result.is_valid);
    assert!(script.calls.lock().is_empty());
    client.close().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn deferred_preflight_rebuilt_client_uses_its_own_gate_after_old_owner_closes() {
    let (old, script) = fixture_options(true, true).await;
    let builder = crate::builder::ClientBuilder::from_client(old.clone())
        .with_allow_offline(Some(true))
        .with_disable_workers(true);
    old.close().await?;
    let rebuilt = builder.build().await?;
    rebuilt.context.api().api_client.check_preflight().await?;
    let lease = rebuilt
        .context
        .incoming_runtime()
        .factory
        .as_ref()
        .unwrap()
        .open(
            [(Topic::new_group_message([4; 16]), Cursor(0))].into(),
            LIMITS,
        )
        .await?;
    assert_eq!(*script.calls.lock(), vec!["configuration", "bidi"]);
    let verifier = rebuilt.scw_verifier();
    let account = xmtp_id::associations::AccountId::new_evm(
        1,
        "0x0000000000000000000000000000000000000001".to_owned(),
    );
    let _ = verifier
        .is_valid_signature(account, [0; 32], Vec::new().into(), None)
        .await;
    assert_eq!(
        script.calls.lock().last(),
        Some(&"verify_smart_contract_wallet_signatures")
    );
    drop(lease);
    rebuilt.close().await?;
}
