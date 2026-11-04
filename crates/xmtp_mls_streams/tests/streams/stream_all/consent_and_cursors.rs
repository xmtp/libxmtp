//! All-message stream tests: consent and cursors.

use super::*;

// verifies: CONS-060
#[xmtp_common::timeout(Duration::from_secs(30))]
#[xmtp_common::test]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn test_stream_all_messages_filters_by_consent_state() {
    tester!(sender, with_name: "sender");
    tester!(receiver, with_name: "receiver");

    for (filter, expected_message) in [
        (ConsentState::Allowed, "msg in allowed"),
        (ConsentState::Denied, "msg in denied"),
        (ConsentState::Unknown, "msg in unknown"),
    ] {
        // A new cohort keeps earlier cursor advances out of this case.
        let allowed_group = sender.create_group(None, None).unwrap();
        let denied_group = sender.create_group(None, None).unwrap();
        let unknown_group = sender.create_group(None, None).unwrap();
        for group in [&allowed_group, &denied_group, &unknown_group] {
            group.add_members(&[receiver.inbox_id()]).await.unwrap();
        }
        for (group, text) in [
            (&allowed_group, "msg in allowed"),
            (&denied_group, "msg in denied"),
            (&unknown_group, "msg in unknown"),
        ] {
            group
                .send_message(text.as_bytes(), SendMessageOpts::default())
                .await
                .unwrap();
        }
        // Sending sets Allowed. Set the final state after all sends.
        denied_group
            .update_consent_state(ConsentState::Denied)
            .unwrap();
        unknown_group
            .update_consent_state(ConsentState::Unknown)
            .unwrap();
        let expected_group = match filter {
            ConsentState::Allowed => allowed_group.group_id,
            ConsentState::Denied => denied_group.group_id,
            ConsentState::Unknown => unknown_group.group_id,
        };
        {
            let stream = sender
                .stream_all_messages(None, Some(vec![filter]))
                .await
                .unwrap();
            futures::pin_mut!(stream);
            loop {
                let item = xmtp_common::time::timeout(Duration::from_secs(5), stream.next())
                    .await
                    .expect("selected item timed out")
                    .expect("stream ended")
                    .expect("stream failed");
                assert_eq!(item.group_id, expected_group, "wrong group for {filter:?}");
                if item.kind == GroupMessageKind::Application {
                    assert_eq!(item.decrypted_message_bytes, expected_message.as_bytes());
                    break;
                }
            }
            // This poll acknowledges the selected item and scans excluded rows.
            assert!(
                xmtp_common::time::timeout(Duration::from_secs(1), stream.next())
                    .await
                    .is_err(),
                "unexpected extra item for {filter:?}"
            );
        }
    }
}

// verifies: CONS-042
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn stream_all_messages_default_excludes_denied() {
    use xmtp_common::time::timeout;

    tester!(alix, disable_workers);
    let allowed = alix.create_group(None, None)?;
    let denied = alix.create_group(None, None)?;
    let unknown = alix.create_group(None, None)?;
    allowed
        .send_message(b"allowed", SendMessageOpts::default())
        .await?;
    denied
        .send_message(b"denied", SendMessageOpts::default())
        .await?;
    unknown
        .send_message(b"unknown", SendMessageOpts::default())
        .await?;
    // Sending changes consent to Allowed. Set the selection after all sends finish.
    denied.update_consent_state(ConsentState::Denied)?;
    unknown.update_consent_state(ConsentState::Unknown)?;

    let selected = LocalDeliveryFilter {
        consent_states: Some(vec![ConsentState::Allowed, ConsentState::Unknown]),
        ..Default::default()
    };
    let snapshot =
        LocalDelivery::history_snapshot(&alix.context, &DeliveryScope::All, &selected, 100)?;
    assert!(
        snapshot
            .messages
            .iter()
            .any(|item| item.message.group_id == unknown.group_id)
    );
    let stream = alix.stream_all_messages(None, None).await?;
    futures::pin_mut!(stream);
    timeout(
        Duration::from_secs(5),
        assert_retained_history(&alix.context, &mut stream, selected),
    )
    .await?;
    assert!(
        timeout(Duration::from_millis(100), stream.next())
            .await
            .is_err()
    );
}

#[xmtp_common::timeout(Duration::from_secs(30))]
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

    let s = stream_all_messages(&alice_2.context, None, None)
        .await
        .unwrap();
    futures::pin_mut!(s);
    alice_group.update_installations().await.unwrap();
    assert_retained_join(&alice_2.context, &mut s, alice_group.group_id).await;

    // The new installation gets its join record, but cannot read the older messages.
    let result = xmtp_common::time::timeout(std::time::Duration::from_secs(1), s.next()).await;
    assert!(matches!(result.unwrap_err(), xmtp_common::time::Expired));

    eve_group
        .send_message(b"decryptable message", SendMessageOpts::default())
        .await
        .unwrap();
    assert_msg!(s, "decryptable message");
}

#[xmtp_common::timeout(Duration::from_secs(20))]
#[rstest::rstest]
#[xmtp_common::test]
async fn test_stream_all_messages_respects_cursor_between_streams() {
    tester!(sender, with_name: "sender");
    tester!(receiver, with_name: "receiver");

    // Step 1: Sender invites receiver to a group
    let group = sender.create_group(None, None).unwrap();
    group.add_members(&[receiver.inbox_id()]).await.unwrap();

    let first_message;
    {
        // Step 2: Create initial stream with no filters
        let stream = receiver.stream_all_messages(None, None).await.unwrap();
        futures::pin_mut!(stream);
        assert_retained_join(&receiver.context, &mut stream, group.group_id).await;

        // Step 3: Sender sends message 1
        group
            .send_message("message 1".as_bytes(), SendMessageOpts::default())
            .await
            .unwrap();

        // Step 4: Receiver gets message 1 from the stream
        first_message = stream.next().await.unwrap().unwrap();
        assert_eq!(first_message.decrypted_message_bytes, b"message 1");

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

        // The last item was not acknowledged by a next-item request before close.
        assert_eq!(new_stream.next().await.unwrap().unwrap(), first_message);
        assert_msg!(new_stream, "message 2");
        assert_msg!(new_stream, "message 3");

        // This request acknowledges message 3. No retained item remains after it.
        let result = xmtp_common::time::timeout(Duration::from_secs(2), new_stream.next()).await;
        assert!(
            result.is_err(),
            "All three application messages must be acknowledged"
        );
    }
}
