//! Tests for the app-backgrounding lifecycle surface: `suspend_streams` /
//! `resume_streams` (the foreground/background pair) and
//! `FfiXmtpClient::catch_up_to_live` (the bounded one-shot catch-up).

use super::streaming::{assert_streamed_history, wait_for_application_messages};
use super::*;
use crate::mls::{FfiCatchUpOptions, resume_streams, suspend_streams};
use crate::stream_failure::{FfiStreamFailureKind, get_stream_failure_details};

/// The Application-kind message payloads in a conversation's durable store, in
/// order. Lets a test assert what catch-up/replay actually wrote to disk — the
/// real bytes — rather than trusting a summary counter.
async fn stored_app_payloads(convo: &FfiConversation) -> Vec<Vec<u8>> {
    convo
        .find_messages(FfiListMessagesOptions::default())
        .await
        .unwrap()
        .into_iter()
        .filter(|m| m.kind == FfiConversationMessageKind::Application)
        .map(|m| m.content)
        .collect()
}

/// A stream survives suspension. Resume replays messages published during
/// suspension from the durable cursor.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 5)]
async fn bidi_suspend_and_resume_redelivers() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // bo owns the group handle; alix joins via its welcome and streams.
    let bo_group = bo
        .conversations()
        .create_group_by_identity(
            vec![alix.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    alix.inner_client.sync_welcomes().await.unwrap();
    let alix_group = alix.conversation(bo_group.id())?;
    let baseline = alix_group.message_history_snapshot(10)?.messages;
    assert_eq!(baseline.len(), 1);
    assert_eq!(
        baseline[0].message.kind,
        FfiConversationMessageKind::MembershipChange
    );
    let mut expected_ids = vec![baseline[0].message.id.clone()];

    let cb = Arc::new(RustStreamCallback::default());
    let stream = alix
        .conversations()
        .stream_all_messages(cb.clone(), None)
        .await;
    stream.wait_for_ready().await;
    xmtp_common::time::timeout(
        Duration::from_secs(15),
        wait_for_eq(
            || async {
                cb.messages
                    .lock()
                    .iter()
                    .map(|message| message.id.clone())
                    .collect::<Vec<_>>()
            },
            expected_ids.clone(),
        ),
    )
    .await??;

    // Baseline: a live message is delivered over the bidi wire.
    let before_id = bo_group
        .send(b"before".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();
    let mut expected = vec![(before_id.clone(), b"before".to_vec())];
    xmtp_common::time::timeout(
        Duration::from_secs(15),
        wait_for_application_messages(&cb, &expected),
    )
    .await?;
    expected_ids.push(before_id);
    let before_history = alix_group
        .message_history_snapshot(10)?
        .messages
        .into_iter()
        .map(|entry| entry.message)
        .collect::<Vec<_>>();
    assert_eq!(
        before_history
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>(),
        expected_ids
    );
    assert_streamed_history(&cb, &before_history);

    // Background: take the shared wire off the network.
    suspend_streams().await.unwrap();

    // Published while suspended — must not reach the stream until resume.
    let during_id = bo_group
        .send(b"during".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    // Check retained messages after the full window, not a cached Notify permit.
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_streamed_history(&cb, &before_history);

    // Foreground: the kept lease reconnects and replays from its durable cursor.
    resume_streams().await.unwrap();
    expected.push((during_id.clone(), b"during".to_vec()));
    xmtp_common::time::timeout(
        Duration::from_secs(15),
        wait_for_application_messages(&cb, &expected),
    )
    .await?;
    expected_ids.push(during_id);

    stream.end_and_wait().await?;
    let history = alix_group
        .message_history_snapshot(10)?
        .messages
        .into_iter()
        .map(|entry| entry.message)
        .collect::<Vec<_>>();
    assert_eq!(
        history
            .iter()
            .map(|message| message.id.clone())
            .collect::<Vec<_>>(),
        expected_ids
    );
    assert_streamed_history(&cb, &history);
    assert_eq!(alix.api_statistics().subscribe_static, 0);
}

/// One-shot catch-up joins a pending group and replays its history from durable
/// cursors, then stops — and a second call finds nothing owed (idempotent).
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 5)]
async fn bidi_catch_up_to_live_replays_and_is_idempotent() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    // bo creates a group with alix and sends. alix has never synced or streamed,
    // so both the welcome and the message are owed.
    let bo_group = bo
        .conversations()
        .create_group_by_identity(
            vec![alix.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    bo_group
        .send(b"missed while away".to_vec(), FfiSendMessageOpts::default())
        .await
        .unwrap();

    let summary = alix.catch_up_to_live(None).await.unwrap();
    assert!(summary.completed);
    assert_eq!(
        summary.conversations, 1,
        "catch-up must join the one pending group"
    );
    assert!(
        summary.messages >= 1,
        "catch-up must replay the missed message"
    );

    let convos = alix
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(convos.len(), 1, "the group is now in alix's store");

    // Catch-up wrote the real payload to disk, not just a counter: the missed
    // text is readable with no further sync.
    let payloads = stored_app_payloads(convos[0].conversation().as_ref()).await;
    assert_eq!(
        payloads,
        vec![b"missed while away".to_vec()],
        "the missed message is stored and readable after catch-up"
    );

    // Nothing owed now: idempotent — zero new on both axes.
    let again = alix.catch_up_to_live(None).await.unwrap();
    assert!(again.completed);
    assert_eq!(
        again.messages, 0,
        "a second catch-up must not replay already-stored messages"
    );
    assert_eq!(
        again.conversations, 0,
        "a second catch-up must not rejoin the already-known group"
    );
}

/// A short deadline can return partial committed progress and unfinished targets.
/// A later full run resumes from durable cursors and stores every expected message.
/// A repeated run then reports no new work.
#[xmtp_common::test(unwrap_try = true, flavor = "multi_thread", worker_threads = 5)]
async fn bidi_catch_up_to_live_bounded_run_is_cancel_safe() {
    let alix = new_test_client().await;
    let bo = new_test_client().await;

    let bo_group = bo
        .conversations()
        .create_group_by_identity(
            vec![alix.account_identifier.clone()],
            FfiCreateGroupOptions::default(),
        )
        .await
        .unwrap();
    // Several messages so a 1ms deadline is likely to land mid-processing.
    for i in 0..5 {
        bo_group
            .send(
                format!("owed {i}").into_bytes(),
                FfiSendMessageOpts::default(),
            )
            .await
            .unwrap();
    }

    // A deadline reports an error with partial committed counts and unfinished targets.
    let bounded = alix
        .catch_up_to_live(Some(FfiCatchUpOptions {
            timeout_ms: Some(1),
        }))
        .await;
    let bounded_messages = match bounded {
        Ok(summary) => {
            assert!(summary.completed);
            summary.messages
        }
        Err(error) => {
            let details = get_stream_failure_details(error.to_string())?;
            assert_eq!(details.kind, FfiStreamFailureKind::CatchUp);
            let summary = details.summary?;
            assert!(!summary.completed);
            assert!(!details.barriers.is_empty());
            summary.messages
        }
    };

    // Whether or not the bounded run was cut off, a full run converges the store
    // from durable cursors — proving the cut left no partial/corrupt state.
    let full = alix.catch_up_to_live(None).await.unwrap();
    assert!(full.completed);
    let convos = alix
        .conversations()
        .list(FfiListConversationsOptions::default())
        .unwrap();
    assert_eq!(convos.len(), 1, "the group converges regardless of the cut");
    let retained = convos[0]
        .conversation()
        .find_messages(FfiListMessagesOptions::default())
        .await?;
    assert!(bounded_messages <= retained.len() as u64);

    // Convergence means the whole history landed intact — all five owed messages,
    // in order, with nothing dropped or duplicated by the cut-off run.
    let payloads = stored_app_payloads(convos[0].conversation().as_ref()).await;
    let expected: Vec<Vec<u8>> = (0..5).map(|i| format!("owed {i}").into_bytes()).collect();
    assert_eq!(
        payloads, expected,
        "all five owed messages converge to the store, in order"
    );

    // And catch-up is now drained: nothing owed on either axis.
    let again = alix.catch_up_to_live(None).await.unwrap();
    assert!(again.completed);
    assert_eq!(again.messages, 0);
    assert_eq!(again.conversations, 0);
}
