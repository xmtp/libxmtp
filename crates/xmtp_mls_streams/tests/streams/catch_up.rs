use xmtp_db::group_message::MsgQueryArgs;
use xmtp_mls::subscriptions::catch_up::CatchUpSummary;
use xmtp_mls::{tester, utils::MlsGroupExt};
/// A client that has never streamed or synced catches up: the pending
/// welcome joins the group (discovery adds an update on the same
/// stream) and the group's history lands in the store.
#[xmtp_common::test(unwrap_try = true)]
async fn catch_up_joins_pending_groups_and_stores_history() {
    tester!(alix);
    tester!(bo);

    let group = bo.create_group(None, None)?;
    group.invite(&alix).await?;
    group.send_msg(b"while you were out").await;
    group.send_msg(b"still out").await;

    let summary = alix.catch_up_to_live(None).await?;

    let alix_group = alix.group(&group.group_id)?;
    let bodies: Vec<Vec<u8>> = alix_group
        .find_messages(&MsgQueryArgs::default())?
        .into_iter()
        .map(|m| m.decrypted_message_bytes)
        .collect();
    assert!(bodies.contains(&b"while you were out".to_vec()));
    assert!(bodies.contains(&b"still out".to_vec()));
    assert_eq!(summary.conversations, 1, "the joined group must be counted");
    assert!(
        summary.messages >= 2,
        "the stored history must be counted (got {})",
        summary.messages
    );
}

/// A member with a stale durable cursor gets the missed tail replayed
/// and stored — and a second run finds nothing new to do.
#[xmtp_common::test(unwrap_try = true)]
async fn catch_up_replays_the_missed_tail_idempotently() {
    tester!(alix);
    tester!(bo);

    let group = alix.create_group(None, None)?;
    group.invite(&bo).await?;
    bo.sync_welcomes().await?;
    let bo_group = bo.group(&group.group_id)?;
    bo_group.sync().await?; // durable cursor at "now"

    group.send_msg(b"missed one").await;
    group.send_msg(b"missed two").await;

    let first = bo.catch_up_to_live(None).await?;
    let count_after_first = bo_group.find_messages(&MsgQueryArgs::default())?.len();
    let bodies: Vec<Vec<u8>> = bo_group
        .find_messages(&MsgQueryArgs::default())?
        .into_iter()
        .map(|m| m.decrypted_message_bytes)
        .collect();
    assert!(bodies.contains(&b"missed one".to_vec()));
    assert!(bodies.contains(&b"missed two".to_vec()));
    assert!(
        first.messages >= 2,
        "the replayed tail must be counted (got {})",
        first.messages
    );
    assert_eq!(first.conversations, 0, "no new group was joined");

    // The honesty proof for the counter: the replay of already-stored
    // history persists nothing, so the summary must say so.
    let second = bo.catch_up_to_live(None).await?;
    let count_after_second = bo_group.find_messages(&MsgQueryArgs::default())?.len();
    assert_eq!(count_after_first, count_after_second);
    assert_eq!(
        second,
        CatchUpSummary {
            completed: true,
            ..Default::default()
        },
        "a second run still reaches live, but persists nothing, so its counts are zero"
    );
}

/// A client with a blocked connection fails catch-up with the cause and sends no request.
// verifies: CONF-075
#[xmtp_common::test(unwrap_try = true)]
async fn blocked_connection_fails_catch_up_before_any_request() {
    use xmtp_mls::subscriptions::catch_up::CatchUpError;
    use xmtp_mls_core::context::CoreContext;
    use xmtp_mls_engine::groups::GroupError;
    use xmtp_mls_server_config::server_configuration::BlockedConnection;
    use xmtp_proto::api::HasStats;
    tester!(alix, disable_workers);
    alix.context
        .server_configuration()
        .block_connection(BlockedConnection::ClientVersionTooOld {
            client: "1.0.0".into(),
            minimum: "9999.0.0".into(),
        });
    let stats = alix.context.api().api_client.mls_stats();
    stats.clear();

    let error = alix.catch_up_to_live(None).await.unwrap_err();

    assert!(
        matches!(
            error,
            CatchUpError::Group(GroupError::Blocked(BlockedConnection::ClientVersionTooOld {
                ref minimum,
                ..
            })) if minimum == "9999.0.0"
        ),
        "expected the blocked connection cause, got {error:?}"
    );
    for (endpoint, count) in [
        ("publish", stats.publish.get_count()),
        ("query", stats.query.get_count()),
        ("query_newest", stats.query_newest.get_count()),
        ("subscribe", stats.subscribe.get_count()),
        ("subscribe_static", stats.subscribe_static.get_count()),
    ] {
        assert_eq!(count, 0, "a blocked client must not call {endpoint}");
    }
}

/// Nothing owed at all — a fresh client's run is just the welcome-topic
/// update acknowledgement and the empty target set.
#[xmtp_common::test(unwrap_try = true)]
async fn catch_up_with_nothing_owed_completes() {
    tester!(alix);
    let summary = alix.catch_up_to_live(None).await?;
    assert_eq!(
        summary,
        CatchUpSummary {
            completed: true,
            ..Default::default()
        },
        "nothing owed still completes, with zero counts"
    );
}
