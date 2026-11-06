//! All-message stream tests: group list.

use super::*;

#[xmtp_common::timeout(Duration::from_secs(25))]
#[rstest::rstest]
#[xmtp_common::test]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_stream_all_messages_changing_group_list() {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let caro_wallet = generate_local_wallet();
    let caro = ClientBuilder::new_test_client_vanilla(&caro_wallet).await;

    let alix_group = alix.create_group(None, None).unwrap();
    tracing::info!("Created alix group {}", hex::encode(alix_group.group_id));
    alix_group.add_members(&[caro.inbox_id()]).await.unwrap();

    {
        let stream = caro.stream_all_messages(None, None).await.unwrap();
        futures::pin_mut!(stream);
        assert_retained_join(&caro.context, &mut stream, alix_group.group_id).await;

        alix_group
            .send_message(b"first", SendMessageOpts::default())
            .await
            .unwrap();
        assert_msg!(stream, "first");
        let bo_group = bo
            .find_or_create_dm_by_identity(caro_wallet.identifier(), None)
            .await
            .unwrap();
        assert_retained_join(&caro.context, &mut stream, bo_group.group_id).await;

        bo_group
            .send_message(b"second", SendMessageOpts::default())
            .await
            .unwrap();
        assert_msg!(stream, "second");

        alix_group
            .send_message(b"third", SendMessageOpts::default())
            .await
            .unwrap();
        assert_msg!(stream, "third");

        let alix_group_2 = alix.create_group(None, None).unwrap();
        alix_group_2.add_members(&[caro.inbox_id()]).await.unwrap();
        assert_retained_join(&caro.context, &mut stream, alix_group_2.group_id).await;

        alix_group
            .send_message(b"fourth", SendMessageOpts::default())
            .await
            .unwrap();
        assert_msg!(stream, "fourth");

        alix_group_2
            .send_message(b"fifth", SendMessageOpts::default())
            .await
            .unwrap();
        assert_msg!(stream, "fifth");
        assert!(
            xmtp_common::time::timeout(Duration::from_millis(200), stream.next())
                .await
                .is_err()
        );
    }

    let caro_known_group = caro.group(&alix_group.group_id).unwrap();
    caro_known_group
        .update_consent_state(ConsentState::Allowed)
        .unwrap();
    let filtered = caro
        .stream_all_messages(None, Some(vec![ConsentState::Allowed]))
        .await
        .unwrap();
    futures::pin_mut!(filtered);
    alix_group
        .send_message(b"known allowed", SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(filtered, "known allowed");

    let new_group = alix.create_group(None, None).unwrap();
    new_group.add_members(&[caro.inbox_id()]).await.unwrap();
    caro.sync_welcomes().await.unwrap();
    let caro_new_group = caro.group(&new_group.group_id).unwrap();
    assert_eq!(
        caro_new_group.consent_state().unwrap(),
        ConsentState::Unknown
    );
    let unknown_message_id = new_group
        .send_message(b"new unknown", SendMessageOpts::default())
        .await
        .unwrap();
    caro_new_group.sync().await.unwrap();
    let stored = caro_new_group
        .find_messages(&MsgQueryArgs::default())
        .unwrap();
    assert!(
        stored.iter().any(|message| {
            message.id == unknown_message_id
                && message.kind == GroupMessageKind::Application
                && message.decrypted_message_bytes == b"new unknown"
        }),
        "new Unknown group message did not reach Caro's local history"
    );
    assert!(
        xmtp_common::time::timeout(Duration::from_secs(2), filtered.next())
            .await
            .is_err(),
        "unknown post-init group entered Allowed stream"
    );
}

#[xmtp_common::timeout(Duration::from_secs(30))]
#[rstest::rstest]
#[xmtp_common::test]
async fn test_dm_stream_all_messages() {
    tester!(alix, with_name: "alix");
    tester!(bo, with_name: "bo");

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();
    let second_known_group = alix.create_group(None, None).unwrap();
    second_known_group
        .add_members(&[bo.inbox_id()])
        .await
        .unwrap();

    let alix_dm = alix.find_or_create_dm(bo.inbox_id(), None).await.unwrap();
    bo.sync_welcomes().await.unwrap();
    {
        // start a stream with only group messages
        let stream = bo
            .stream_all_messages(Some(ConversationType::Group), None)
            .await
            .unwrap();
        futures::pin_mut!(stream);
        assert_retained_history(
            &bo.context,
            &mut stream,
            LocalDeliveryFilter {
                conversation_type: Some(ConversationType::Group),
                consent_states: None,
            },
        )
        .await;
        alix_dm
            .send_message("first DM msg".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        let first_group_message_id = alix_group
            .send_message("first GROUP msg".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        let second_group_message_id = second_known_group
            .send_message(
                "second known GROUP msg".as_bytes(),
                SendMessageOpts::default(),
            )
            .await
            .unwrap();
        // The two groups can deliver in either order.
        let expected = HashSet::from([
            (
                alix_group.group_id,
                first_group_message_id,
                b"first GROUP msg".to_vec(),
            ),
            (
                second_known_group.group_id,
                second_group_message_id,
                b"second known GROUP msg".to_vec(),
            ),
        ]);
        let received = [
            stream.next().await.unwrap().unwrap(),
            stream.next().await.unwrap().unwrap(),
        ]
        .into_iter()
        .map(|message| {
            assert_eq!(message.kind, GroupMessageKind::Application);
            (
                message.group_id,
                message.id,
                message.decrypted_message_bytes,
            )
        })
        .collect::<HashSet<_>>();
        assert_eq!(received, expected);
        bo.sync_all_welcomes_and_groups(None).await.unwrap();
        assert!(
            xmtp_common::time::timeout(Duration::from_secs(1), stream.next())
                .await
                .is_err()
        );
    }
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    {
        // Start a stream with only dms
        let stream = bo
            .stream_all_messages(Some(ConversationType::Dm), None)
            .await
            .unwrap();
        futures::pin_mut!(stream);
        alix_group
            .send_message("second GROUP msg".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        alix_dm
            .send_message("second DM msg".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        assert_msg!(stream, "second DM msg");
        let new_group = alix.create_group(None, None).unwrap();
        new_group.add_members(&[bo.inbox_id()]).await.unwrap();
        new_group
            .send_message("new GROUP msg".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        bo.sync_all_welcomes_and_groups(None).await.unwrap();
        assert!(
            xmtp_common::time::timeout(Duration::from_secs(1), stream.next())
                .await
                .is_err()
        );
    }
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    // Both filtered streams acknowledged their messages and scanned excluded rows.
    let stream = bo.stream_all_messages(None, None).await.unwrap();
    futures::pin_mut!(stream);
    alix_group
        .send_message("first".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(stream, "first");

    alix_dm
        .send_message("second".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(stream, "second");
}

#[xmtp_common::timeout(Duration::from_secs(60))]
#[rstest::rstest]
#[xmtp_common::test]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_stream_all_messages_does_not_lose_messages() {
    let caro = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let alix = Arc::new(ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await);
    let eve = Arc::new(ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await);
    let bo = Arc::new(ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await);

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group
        .add_members(&[caro.inbox_id(), bo.inbox_id()])
        .await
        .unwrap();

    let bo_group = bo.sync_welcomes().await.unwrap()[0].clone();

    let mut stream = caro.stream_all_messages(None, None).await.unwrap();

    let alix_group_pointer = alix_group.clone();
    xmtp_common::spawn(None, async move {
        for i in 0..15 {
            let msg = format!("main spam {i}");
            alix_group_pointer
                .send_message(msg.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
            xmtp_common::time::sleep(Duration::from_micros(100)).await;
        }
    });

    // Eve will try to break our stream by creating lots of groups
    // and immediately sending a message
    // this forces our streams to re-subscribe
    let caro_id = caro.inbox_id().to_string();
    xmtp_common::spawn(None, async move {
        let caro = &caro_id;
        for i in 0..15 {
            let new_group = eve.create_group(None, None).unwrap();
            new_group.add_members(&[caro]).await.unwrap();
            let msg = format!("EVE spam {i} from new group");
            new_group
                .send_message(msg.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
        }
    });

    // Bo will try to break our stream by sending lots of messages
    // this forces our streams to handle resubscribes while receiving lots of messages
    xmtp_common::spawn(None, async move {
        let bo_group = &bo_group;
        for i in 0..15 {
            bo_group
                .send_message(format!("bo msg {i}").as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
            xmtp_common::time::sleep(Duration::from_millis(50)).await
        }
    });

    let mut messages = Vec::new();
    let timeout = if cfg!(target_arch = "wasm32") {
        Duration::from_secs(20)
    } else {
        Duration::from_secs(10)
    };
    loop {
        tokio::select! {
            Some(msg) = stream.next() => {
                match msg {
                    Ok(m) => messages.push(m),
                    Err(e) => {
                        panic!("stream failed: {e}");
                    }
                }
            },
            _ = xmtp_common::time::sleep(timeout) => break
        }
    }

    let ids: HashSet<_> = messages.iter().map(|message| &message.id).collect();
    assert_eq!(ids.len(), messages.len(), "duplicate message IDs");
    let application_messages: Vec<_> = messages
        .iter()
        .filter(|message| message.kind == GroupMessageKind::Application)
        .map(|message| String::from_utf8(message.decrypted_message_bytes.clone()).unwrap())
        .collect();
    let expected: HashSet<_> = (0..15)
        .flat_map(|i| {
            [
                format!("main spam {i}"),
                format!("EVE spam {i} from new group"),
                format!("bo msg {i}"),
            ]
        })
        .collect();
    assert_eq!(application_messages.len(), 45);
    assert_eq!(
        application_messages.into_iter().collect::<HashSet<_>>(),
        expected
    );
    let memberships: Vec<_> = messages
        .iter()
        .filter(|message| message.kind == GroupMessageKind::MembershipChange)
        .collect();
    assert_eq!(memberships.len(), 16);
    assert_eq!(
        memberships
            .iter()
            .map(|message| message.group_id)
            .collect::<HashSet<_>>()
            .len(),
        16,
    );
    let history = LocalDelivery::history_snapshot(
        &caro.context,
        &DeliveryScope::All,
        &LocalDeliveryFilter::default(),
        100,
    )
    .unwrap();
    assert_eq!(
        messages,
        history
            .messages
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>()
    );
}

#[xmtp_common::timeout(Duration::from_secs(20))]
#[rstest::rstest]
#[xmtp_common::test]
async fn test_stream_all_messages_detached_group_changes() {
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let hale = Arc::new(ClientBuilder::new_test_client(&generate_local_wallet()).await);
    let stream = caro.stream_all_messages(None, None).await.unwrap();

    let caro_id = caro.inbox_id().to_string();
    xmtp_common::spawn(None, async move {
        let caro = &caro_id;
        for i in 0..5 {
            let new_group = hale.create_group(None, None).unwrap();
            new_group.add_members(&[caro]).await.unwrap();
            tracing::info!(
                "\n\n HALE SENDING {i} to group {}\n\n",
                hex::encode(new_group.group_id)
            );
            new_group
                .send_message(b"spam from new group", SendMessageOpts::default())
                .await
                .unwrap();
        }
    });

    let mut messages = Vec::new();
    let _ = xmtp_common::time::timeout(Duration::from_secs(20), async {
        futures::pin_mut!(stream);
        loop {
            if messages.len() < 5 {
                if let Some(Ok(msg)) = stream.next().await {
                    tracing::info!(
                        message_id = hex::encode(&msg.id),
                        sender_inbox_id = msg.sender_inbox_id,
                        sender_installation_id = hex::encode(&msg.sender_installation_id),
                        group_id = %msg.group_id,
                        "GOT MESSAGE {}, text={}",
                        messages.len(),
                        String::from_utf8_lossy(msg.decrypted_message_bytes.as_slice())
                    );
                    messages.push(msg)
                }
            } else {
                break;
            }
        }
    })
    .await;
    tracing::info!("Total Messages: {}", messages.len());
    assert_eq!(messages.len(), 5);
}
