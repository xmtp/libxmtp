//! All-message stream tests: concurrency.

use super::*;

#[xmtp_common::timeout(Duration::from_secs(60))]
#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread")]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_stream_all_concurrent_writes() {
    // Create test clients
    tester!(alix, with_name: "alix");
    tester!(bo, with_name: "bo");
    tester!(caro, with_name: "caro");
    tester!(davon, with_name: "davon");

    let spacing_time = Duration::from_millis(50);

    // Create two groups
    let alix_group = alix
        .create_group_with_members(&[caro.inbox_id(), bo.inbox_id()], None, None)
        .await
        .unwrap();

    let caro_group_2 = caro
        .create_group_with_members(&[alix.inbox_id(), bo.inbox_id()], None, None)
        .await
        .unwrap();

    // Sync welcomes for all clients
    alix.sync_welcomes().await.unwrap();
    caro.sync_welcomes().await.unwrap();
    bo.sync_welcomes().await.unwrap();

    // Get group references for each client
    let bo_group = bo.group(&alix_group.group_id).unwrap();
    let bo_group_2 = bo.group(&caro_group_2.group_id).unwrap();
    let caro_group = caro.group(&alix_group.group_id).unwrap();
    let alix_group_2 = alix.group(&caro_group_2.group_id).unwrap();

    // Track all sent messages
    let sent_messages = Arc::new(tokio::sync::Mutex::new(HashSet::new()));

    // Start Caro's message stream
    let mut stream = caro.stream_all_messages(None, None).await.unwrap();

    // Give the stream a moment to initialize
    xmtp_common::time::sleep(Duration::from_millis(250)).await;

    // Spawn Alix's message sending task
    let alix_group_clone = alix_group.clone();
    let alix_group_2_clone = alix_group_2.clone();
    let sent_messages_alix = sent_messages.clone();
    xmtp_common::spawn(None, async move {
        for i in 0..20 {
            let message = format!("Alix Message {}", i);
            sent_messages_alix.lock().await.insert(message.clone());
            alix_group_clone
                .send_message(message.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
            let message = format!("Alix2 Message {}", i);
            sent_messages_alix.lock().await.insert(message.clone());

            alix_group_2_clone
                .send_message(message.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
            xmtp_common::time::sleep(spacing_time).await;
        }
    });

    // Spawn Bo's message sending task
    let bo_group_clone = bo_group.clone();
    let bo_group_2_clone = bo_group_2.clone();
    let sent_messages_bo = sent_messages.clone();
    xmtp_common::spawn(None, async move {
        for i in 0..10 {
            let message = format!("Bo Message {}", i);
            sent_messages_bo.lock().await.insert(message.clone());
            bo_group_clone
                .send_message(message.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();

            let message = format!("Bo2 Message {}", i);
            sent_messages_bo.lock().await.insert(message.clone());
            bo_group_2_clone
                .send_message(message.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
            xmtp_common::time::sleep(spacing_time).await;
        }
    });

    // Spawn Davon's spam group creation task
    let caro_inbox_id = caro.inbox_id().to_string();
    let sent_messages_davon = sent_messages.clone();
    xmtp_common::spawn(None, async move {
        for i in 0..20 {
            let spam_message = format!("Davon Spam Message {}", i);
            let group = davon
                .create_group_with_members(&[&caro_inbox_id], None, None)
                .await
                .unwrap();

            group
                .send_message(spam_message.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();

            sent_messages_davon.lock().await.insert(spam_message);
            xmtp_common::time::sleep(spacing_time).await;
        }
    });

    // Spawn Caro's message sending task
    let caro_group_clone = caro_group.clone();
    let caro_group_2_clone = caro_group_2.clone();
    let sent_messages_caro = sent_messages.clone();
    xmtp_common::spawn(None, async move {
        for i in 0..10 {
            let message = format!("Caro Message {}", i);
            sent_messages_caro.lock().await.insert(message.clone());
            caro_group_clone
                .send_message(message.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
            let message = format!("Caro2 Message {}", i);
            sent_messages_caro.lock().await.insert(message.clone());
            caro_group_2_clone
                .send_message(message.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
            xmtp_common::time::sleep(spacing_time).await;
        }
    });

    // Collect messages from the stream
    let mut messages = Vec::new();
    let timeout = if cfg!(target_arch = "wasm32") {
        Duration::from_secs(30)
    } else {
        Duration::from_secs(10)
    };
    loop {
        tokio::select! {
            Some(msg) = stream.next() => {
                match msg {
                    Ok(m) => {
                        if m.kind == GroupMessageKind::Application {
                            tracing::info!(
                                "Received message {} (#{})",
                                String::from_utf8_lossy(&m.decrypted_message_bytes),
                                messages.len()
                            );
                            messages.push(m);
                        }
                    }
                    Err(e) => {
                        tracing::error!("error in stream test {e}");
                    }
                }
            },
            _ = xmtp_common::time::sleep(timeout) => break
        }
    }

    let groups = [
        ("bo", &bo_group),
        ("alix", &alix_group),
        ("caro", &caro_group),
    ];
    let each_group_message_count = 41;

    for (name, group) in groups {
        group.sync().await.unwrap();
        assert_eq!(
            group.find_messages(&MsgQueryArgs::default()).unwrap().len(),
            each_group_message_count,
            "{}'s group should have {} messages (40 messages + 1 membership change)",
            name,
            each_group_message_count
        );
    }

    // Compare sent vs received messages
    let sent_messages = sent_messages.lock().await;
    let received_messages: HashSet<String> = messages
        .iter()
        .map(|m| String::from_utf8_lossy(&m.decrypted_message_bytes).to_string())
        .collect();

    assert_eq!(
        sent_messages.len(),
        100,
        "100 messages should have been sent"
    );

    // Find missing messages (sent but not received)
    let missing_messages: Vec<_> = sent_messages.difference(&received_messages).collect();

    // Find unexpected messages (received but not sent)
    let unexpected_messages: Vec<_> = received_messages.difference(&*sent_messages).collect();

    if !missing_messages.is_empty() {
        tracing::error!(
            "Missing {} messages that were sent but not received:",
            missing_messages.len()
        );
        for msg in &missing_messages {
            tracing::error!("  - {}", msg);
        }
    }

    if !unexpected_messages.is_empty() {
        tracing::error!(
            "Found {} unexpected messages that were received but not sent:",
            unexpected_messages.len()
        );
        for msg in &unexpected_messages {
            tracing::error!("  - {}", msg);
        }
    }

    // Verify all sent messages were received
    assert!(
        missing_messages.is_empty(),
        "Missing {} messages: {:?}",
        missing_messages.len(),
        missing_messages
    );

    let num_received = received_messages.len();
    let num_sent = sent_messages.len();

    assert!(
        num_received == num_sent,
        "Received {} messages. Expected {} (40 from Alix + 20 from Bo + 10 from Davon + 20 from Caro)",
        num_received,
        num_sent
    );
}

#[xmtp_common::timeout(Duration::from_secs(30))]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_new_group_does_not_duplicate_messages() {
    tester!(alix);
    tester!(bo);

    // Create 250 groups with both accounts and send one message to each
    let mut initial_groups = Vec::with_capacity(50);
    for i in 0..50 {
        let group = alix.create_group(Default::default(), Default::default())?;
        group.add_members(&[bo.inbox_id()]).await?;
        group
            .send_message(
                format!("Initial message {}", i).as_bytes(),
                Default::default(),
            )
            .await?;
        initial_groups.push(group);
    }

    let mut stream = alix
        .stream_all_messages_owned_with_stats(None, None)
        .await?;
    let stats = stream.stats();

    // Create a new group to trigger a reconnect
    let _ = alix.create_group(Default::default(), Default::default())?;

    xmtp_common::spawn(
        None,
        async move { while (stream.next().await).is_some() {} },
    );

    xmtp_common::time::sleep(Duration::from_secs(10)).await;

    let new_stats = stats.new_stats().await;

    assert!(
        new_stats.len() < 5,
        "Stream has processed {} messages when expected to have processed 1",
        new_stats.len()
    );
}
