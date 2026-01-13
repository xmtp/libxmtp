use super::*;
use crate::groups::send_message_opts::SendMessageOpts;

#[cfg(target_arch = "wasm32")]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

use crate::subscriptions::stream_messages::stream_stats::StreamWithStats;
use crate::tester;
use crate::{assert_msg, builder::ClientBuilder};
use futures::StreamExt;
use rstest::*;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use xmtp_cryptography::utils::generate_local_wallet;
use xmtp_db::group_message::{GroupMessageKind, MsgQueryArgs};
use xmtp_id::associations::test_utils::WalletTestExt;

#[rstest::rstest]
#[xmtp_common::test]
#[timeout(Duration::from_secs(15))]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_stream_all_messages_changing_group_list() {
    let alix = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let caro_wallet = generate_local_wallet();
    let caro = ClientBuilder::new_test_client_vanilla(&caro_wallet).await;

    let alix_group = alix.create_group(None, None).unwrap();
    tracing::info!("Created alix group {}", hex::encode(&alix_group.group_id));
    alix_group.add_members(&[caro.inbox_id()]).await.unwrap();

    let stream = caro.stream_all_messages(None, None).await.unwrap();
    futures::pin_mut!(stream);

    alix_group
        .send_message(b"first", SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(stream, "first");
    let bo_group = bo
        .find_or_create_dm_by_identity(caro_wallet.identifier(), None)
        .await
        .unwrap();

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
}

#[rstest::rstest]
#[xmtp_common::test]
#[timeout(Duration::from_secs(15))]
async fn test_stream_all_messages_unchanging_group_list() {
    let alix = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let bo = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let caro = ClientBuilder::new_test_client(&generate_local_wallet()).await;

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[caro.inbox_id()]).await.unwrap();

    let bo_group = bo.create_group(None, None).unwrap();
    bo_group.add_members(&[caro.inbox_id()]).await.unwrap();

    let stream = caro.stream_all_messages(None, None).await.unwrap();
    futures::pin_mut!(stream);
    bo_group
        .send_message(b"first", SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(stream, "first");

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

    bo_group
        .send_message(b"fourth", SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(stream, "fourth");
}

#[rstest::rstest]
#[xmtp_common::test]
async fn test_dm_stream_all_messages() {
    tester!(alix, with_name: "alix");
    tester!(bo, with_name: "bo");

    let alix_group = alix.create_group(None, None).unwrap();
    alix_group.add_members(&[bo.inbox_id()]).await.unwrap();

    let alix_dm = alix.find_or_create_dm(bo.inbox_id(), None).await.unwrap();
    {
        // start a stream with only group messages
        let stream = bo
            .stream_all_messages(Some(ConversationType::Group), None)
            .await
            .unwrap();
        futures::pin_mut!(stream);
        alix_dm
            .send_message("first DM msg".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        alix_group
            .send_message("first GROUP msg".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();
        assert_msg!(stream, "first GROUP msg");
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
    }
    bo.sync_all_welcomes_and_groups(None).await.unwrap();
    // Start a stream with all conversations
    // Wait for 2 seconds for the group creation to be streamed
    let stream = bo.stream_all_messages(None, None).await.unwrap();
    futures::pin_mut!(stream);
    alix_group
        .send_message("first".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    // TODO:d14n
    // this discrepancy is because of the LCC (we get duplicates)
    // not sure if theres an easy fix
    // https://github.com/xmtp/libxmtp/issues/2613
    if cfg!(feature = "d14n") {
        assert_msg!(stream, "second DM msg");
    }
    assert_msg!(stream, "first");

    alix_dm
        .send_message("second".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(stream, "second");
}

use std::collections::HashMap;
fn find_duplicates_with_count(strings: &[String]) -> HashMap<&String, usize> {
    let mut counts = HashMap::new();

    // Count occurrences
    for string in strings {
        *counts.entry(string).or_insert(0) += 1;
    }

    // Filter to keep only strings that appear more than once
    counts.retain(|_, count| *count > 1);

    counts
}

#[ignore]
#[rstest::rstest]
#[xmtp_common::test]
#[timeout(Duration::from_secs(60))]
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
                        tracing::error!("error in stream test {e}");
                    }
                }
            },
            _ = xmtp_common::time::sleep(timeout) => break
        }
    }

    let msgs = &messages
        .iter()
        .map(|m| String::from_utf8_lossy(m.decrypted_message_bytes.as_slice()).to_string())
        .collect::<Vec<String>>();
    let duplicates = find_duplicates_with_count(msgs);
    assert!(duplicates.is_empty());
    assert_eq!(
        messages.len(),
        45,
        "too many messages mean duplicates, too little means missed. Also ensure timeout is sufficient."
    );
}

#[rstest::rstest]
#[xmtp_common::test]
#[timeout(Duration::from_secs(20))]
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
                hex::encode(&new_group.group_id)
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
                        group_id = hex::encode(&msg.group_id),
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

#[rstest::rstest]
#[case(ConsentState::Allowed, "msg in allowed")]
#[case(ConsentState::Denied, "msg in denied")]
#[case(ConsentState::Unknown, "msg in unknown")]
#[xmtp_common::test]
#[timeout(Duration::from_secs(20))]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_stream_all_messages_filters_by_consent_state(
    #[case] filter: ConsentState,
    #[case] expected_message: &str,
) {
    tester!(sender, with_name: "sender");
    tester!(receiver, with_name: "receiver");

    // Create group with Allowed consent
    let allowed_group = sender.create_group(None, None).unwrap();
    allowed_group
        .add_members(&[receiver.inbox_id()])
        .await
        .unwrap();

    // Create group with Denied consent
    let denied_group = sender.create_group(None, None).unwrap();
    denied_group
        .add_members(&[receiver.inbox_id()])
        .await
        .unwrap();
    denied_group
        .update_consent_state(ConsentState::Denied)
        .unwrap();

    // Create group with Unknown consent
    let unknown_group = sender.create_group(None, None).unwrap();
    unknown_group
        .add_members(&[receiver.inbox_id()])
        .await
        .unwrap();
    unknown_group
        .update_consent_state(ConsentState::Unknown)
        .unwrap();

    sender.sync_welcomes().await.unwrap();
    xmtp_common::time::sleep(Duration::from_millis(100)).await;

    let stream = sender
        .stream_all_messages(None, Some(vec![filter]))
        .await
        .unwrap();
    futures::pin_mut!(stream);
    //  if cfg!(feature = "d14n") {
    //      // group updated codec b/c group hasn't written to db so lcc is 0
    //      use futures_test::assert_stream_next;
    //      let _ = stream.next().await.unwrap();
    //  }
    allowed_group
        .send_message("msg in allowed".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    denied_group
        .send_message("msg in denied".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    unknown_group
        .send_message("msg in unknown".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    assert_msg!(stream, expected_message);
}

#[rstest]
#[xmtp_common::test]
async fn stream_messages_keeps_track_of_cursor() {
    tester!(bo, with_name: "bo");
    tester!(eve, with_name: "eve");
    tester!(alice, with_name: "alice");
    let alice_group = alice.create_group(None, None).unwrap();

    alice_group
        .add_members(&[bo.inbox_id(), eve.inbox_id()])
        .await
        .unwrap();
    let _bo_groups = bo.sync_welcomes().await.unwrap();
    let eve_groups = eve.sync_welcomes().await.unwrap();
    let eve_group = eve_groups.first().unwrap();
    alice_group.sync().await.unwrap();
    // get the group epoch to 28
    for _ in 0..7 {
        alice_group
            .update_group_name(format!("test name {}", xmtp_common::rand_string::<5>()))
            .await
            .unwrap();
    }
    for _ in 0..25 {
        eve_group
            .send_message(
                format!("message {}", xmtp_common::rand_string::<5>()).as_bytes(),
                SendMessageOpts::default(),
            )
            .await
            .unwrap();
    }
    // get the group epoch to 28
    for _ in 0..7 {
        alice_group
            .update_group_name(format!("test name {}", xmtp_common::rand_string::<5>()))
            .await
            .unwrap();
    }
    alice_group.sync().await.unwrap();

    /////////////////////////////////// New installation \\\\\\\\\\\\\\\\\\\\\\\\\
    //                   create new installation for alice                      \\
    /////////////////////////////////////////////////////\\\\\\\\\\\\\\\\\\\\\\\\\

    tester!(alice_2, from: alice);

    let mut s = StreamAllMessages::new(&alice_2.context, None, None)
        .await
        .unwrap();
    // elapse enough time to update installations
    xmtp_common::time::sleep(std::time::Duration::from_secs(2)).await;
    alice_group.update_installations().await.unwrap();
    // if the stream behaved as expected, it should have set the cursor to the latest
    // in the group before any messages that could actually be decrypted by alices
    // second installation were sent.

    // we should timeout because we have not gotten a decryptable message yet.
    let result = xmtp_common::time::timeout(std::time::Duration::from_secs(1), s.next()).await;
    assert!(matches!(result.unwrap_err(), xmtp_common::time::Expired));

    eve_group
        .send_message(b"decryptable message", SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(s, "decryptable message");
}

#[rstest::rstest]
#[xmtp_common::test]
#[timeout(Duration::from_secs(20))]
async fn test_stream_all_messages_filters_conversations_created_after_init() {
    let sender = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;
    let receiver = ClientBuilder::new_test_client_vanilla(&generate_local_wallet()).await;

    // Start stream filtering for only "allowed" conversations
    let stream = receiver
        .stream_all_messages(None, Some(vec![ConsentState::Allowed]))
        .await
        .unwrap();
    futures::pin_mut!(stream);

    // Create new group that will arrive via conversation stream
    let new_group = sender.create_group(None, None).unwrap();
    new_group.add_members(&[receiver.inbox_id()]).await.unwrap();

    new_group
        .send_message(b"new message", SendMessageOpts::default())
        .await
        .unwrap();
    // Verify that no unknown message was received
    let result = xmtp_common::time::timeout(Duration::from_secs(2), stream.next()).await;
    assert!(
        result.is_err(),
        "Should not receive messages from unknown consent group"
    );
}

#[rstest::rstest]
#[xmtp_common::test]
#[timeout(Duration::from_secs(20))]
async fn test_stream_all_messages_filters_new_group_when_dm_only() {
    let sender = ClientBuilder::new_test_client(&generate_local_wallet()).await;
    let receiver_wallet = generate_local_wallet();
    let receiver = ClientBuilder::new_test_client(&receiver_wallet).await;

    // Create initial DM
    let dm = sender
        .find_or_create_dm_by_identity(receiver_wallet.identifier(), None)
        .await
        .unwrap();

    receiver.sync_welcomes().await.unwrap();
    xmtp_common::time::sleep(Duration::from_millis(100)).await;

    // Start stream filtering for only DM conversations
    let stream = receiver
        .stream_all_messages(Some(ConversationType::Dm), None)
        .await
        .unwrap();
    futures::pin_mut!(stream);

    // Send message in DM - should appear in stream
    dm.send_message("msg in dm".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(stream, "msg in dm");

    // Create new group that will arrive via conversation stream
    let new_group = sender.create_group(None, None).unwrap();
    new_group.add_members(&[receiver.inbox_id()]).await.unwrap();

    // Send message in group - should NOT appear in stream
    new_group
        .send_message("msg in group".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    // Verify that no group message was received
    let result = xmtp_common::time::timeout(Duration::from_secs(1), stream.next()).await;
    assert!(
        result.is_err(),
        "Should not receive messages from group conversations when filtering for DMs"
    );
}

#[rstest::rstest]
#[xmtp_common::test]
#[timeout(Duration::from_secs(20))]
async fn test_stream_all_messages_respects_cursor_between_streams() {
    tester!(sender, with_name: "sender");
    tester!(receiver, with_name: "receiver");

    // Step 1: Sender invites receiver to a group
    let group = sender.create_group(None, None).unwrap();
    group.add_members(&[receiver.inbox_id()]).await.unwrap();

    {
        // Step 2: Create initial stream with no filters
        let stream = receiver.stream_all_messages(None, None).await.unwrap();
        futures::pin_mut!(stream);

        // Step 3: Sender sends message 1
        group
            .send_message("message 1".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();

        // Step 4: Receiver gets message 1 from the stream
        assert_msg!(stream, "message 1");

        // Step 5: Close the stream by dropping it
    }

    // Step 6: Sender sends message 2 while stream is closed
    group
        .send_message("message 2".as_bytes(), SendMessageOpts::default())
        .await
        .unwrap();

    {
        // Step 7: Open a new stream
        let new_stream = receiver.stream_all_messages(None, None).await.unwrap();
        futures::pin_mut!(new_stream);

        // Step 8: Sender sends message 3
        group
            .send_message("message 3".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();

        // Verify: The new stream should receive messages 2 and 3
        assert_msg!(new_stream, "message 2");
        assert_msg!(new_stream, "message 3");

        // Verify that message 1 is not received a second time
        let result = xmtp_common::time::timeout(Duration::from_secs(2), new_stream.next()).await;
        assert!(
            result.is_err(),
            "Should not receive message 1 which was previously processed"
        );
    }
}

#[rstest::rstest]
#[xmtp_common::test(flavor = "multi_thread")]
#[timeout(Duration::from_secs(60))]
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
