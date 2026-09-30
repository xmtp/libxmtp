//! Barrier-based syncs when the connection is blocked while they run.
use super::*;
use crate::groups::GroupError;
use crate::server_configuration::BlockedConnection;
use crate::subscriptions::barrier::{BarrierError, BarrierFailure};
use crate::tester;
use crate::utils::test::register_client;
use xmtp_cryptography::utils::generate_local_wallet;

/// An offline client on another deployment, and push envelopes for work it
/// has not processed: a Welcome and a message in a group it joined.
struct Mismatched {
    client: TestClient2,
    group_id: xmtp_proto::types::GroupId,
    welcome: Vec<u8>,
    group_message: Vec<u8>,
}

/// A client created online, whose stored configuration then names another
/// deployment, built again offline. Its first request fails the deferred
/// deployment check, which blocks the connection and cancels the client.
async fn offline_client_on_another_deployment() -> Result<Mismatched, Box<dyn std::error::Error>> {
    use crate::groups::send_message_opts::SendMessageOpts;
    use xmtp_proto::types::{Cursor, Topic};

    let owner = generate_local_wallet();
    let path = xmtp_common::tmp_path();
    let first = Client::builder(crate::utils::test::identity_setup(&owner))
        .store(TestDb::create_persistent_store(Some(path.clone())).await)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .enable_sqlite_triggers()
        .default_mls_store()?
        .local()
        .device_sync_worker_mode(crate::builder::DeviceSyncMode::Disabled)
        .with_disable_workers(true)
        .build()
        .await?;
    register_client(&first, &owner).await;
    tester!(bo, disable_workers);
    let joined = bo
        .create_group_with_members(&[first.inbox_id()], None, None)
        .await?;
    first.sync_welcomes().await?;
    // Work after the last sync, so the offline client must fetch it.
    bo.create_group_with_members(&[first.inbox_id()], None, None)
        .await?;
    joined
        .send_message(b"after the last sync", SendMessageOpts::default())
        .await?;
    let newest = |topic: Topic| {
        let api = first.context.api();
        async move {
            let envelopes = api
                .query_all(
                    [(topic, Cursor(0))].into(),
                    api.limits().max_query_limit as u32,
                )
                .await?;
            Ok::<_, Box<dyn std::error::Error>>(
                envelopes.last().expect("an envelope").encode_to_vec(),
            )
        }
    };
    let welcome = newest(Topic::new_welcome_message(first.context.installation_id())).await?;
    let group_message = newest(Topic::new_group_message(joined.group_id)).await?;
    let stored = first.context.db().server_configuration()?.unwrap();
    first.context.db().store_server_configuration(
        "org.example.other-deployment",
        "http://moved.example",
        &stored.response,
        stored.fetched_at_ns,
    )?;
    first.close().await?;
    drop(first);

    let client = Client::builder(IdentityStrategy::CachedOnly)
        .store(TestDb::create_persistent_store(Some(path)).await)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .default_mls_store()?
        .local()
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    Ok(Mismatched {
        client,
        group_id: joined.group_id,
        welcome,
        group_message,
    })
}

type TestClient2 = crate::utils::FullXmtpClient;

/// Each entry point that waits on the processing barrier, called first.
#[derive(Clone, Copy, Debug)]
enum EntryPoint {
    SyncWelcomes,
    SyncAllGroups,
    SyncAllWelcomesAndGroups,
    GroupSync,
    CatchUp,
    PushWelcome,
    PushGroupMessage,
}

fn backend_mismatch(error: &GroupError) -> bool {
    matches!(
        error,
        GroupError::Client(ClientError::BackendMismatch { .. })
    )
}

/// The first request of each barrier-based entry point fails the deferred
/// deployment check. The call reports that check's error, as a direct
/// request does, not the barrier or the close it caused.
// verifies: CONF-077
#[rstest::rstest]
#[case::sync_welcomes(EntryPoint::SyncWelcomes)]
#[case::sync_all_groups(EntryPoint::SyncAllGroups)]
#[case::sync_all_welcomes_and_groups(EntryPoint::SyncAllWelcomesAndGroups)]
#[case::group_sync(EntryPoint::GroupSync)]
#[case::catch_up(EntryPoint::CatchUp)]
#[case::push_welcome(EntryPoint::PushWelcome)]
#[case::push_group_message(EntryPoint::PushGroupMessage)]
#[xmtp_common::test(unwrap_try = true)]
async fn first_barrier_call_on_another_deployment_reports_the_backend_mismatch(
    #[case] entry: EntryPoint,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::subscriptions::SubscribeError;
    use crate::subscriptions::catch_up::CatchUpError;

    let Mismatched {
        client,
        group_id,
        welcome,
        group_message,
    } = offline_client_on_another_deployment().await?;
    let group = client.group(&group_id)?;
    let reported = match entry {
        EntryPoint::SyncWelcomes => client
            .sync_welcomes()
            .await
            .err()
            .is_some_and(|error| backend_mismatch(&error)),
        EntryPoint::SyncAllGroups => client
            .sync_all_groups(vec![group])
            .await
            .err()
            .is_some_and(|error| backend_mismatch(&error)),
        EntryPoint::SyncAllWelcomesAndGroups => client
            .sync_all_welcomes_and_groups(None)
            .await
            .err()
            .is_some_and(|error| backend_mismatch(&error)),
        EntryPoint::GroupSync => group
            .sync()
            .await
            .err()
            .is_some_and(|error| backend_mismatch(&error)),
        EntryPoint::CatchUp => matches!(
            client.catch_up_to_live(None).await,
            Err(CatchUpError::Group(error)) if backend_mismatch(&error)
        ),
        EntryPoint::PushWelcome => matches!(
            client.process_streamed_welcome_message(welcome).await,
            Err(SubscribeError::Group(error)) if backend_mismatch(&error)
        ),
        EntryPoint::PushGroupMessage => matches!(
            group.process_streamed_group_message(group_message).await,
            Err(SubscribeError::Group(error)) if backend_mismatch(&error)
        ),
    };
    assert!(
        reported,
        "{entry:?} did not report the deployment check's BackendMismatch"
    );
    Ok(())
}

/// Start a sync on an online client and stop it after it passed its
/// entry check: with `block`, the way the configuration worker blocks the
/// connection; without it, the way `end()` cancels the client.
async fn sync_stopped_while_running(block: bool) -> GroupError {
    tester!(alix, disable_workers);
    // An unconfirmed publish gives the sync a group barrier to wait on.
    alix.create_group(None, None).unwrap();
    let sync = alix.sync_all_welcomes_and_groups(None);
    futures::pin_mut!(sync);
    assert!(
        futures::poll!(&mut sync).is_pending(),
        "the sync finished before it could be stopped"
    );
    if block {
        alix.context
            .server_configuration()
            .block_connection(BlockedConnection::BackendMismatch {
                stored: "org.example.stored".into(),
                received: "org.example.other".into(),
            });
    }
    alix.context.cancellation_token().cancel();
    sync.await.expect_err("a stopped sync cannot complete")
}

/// A block that happens after the targets were captured reports the block.
// verifies: CONF-075
#[xmtp_common::test(unwrap_try = true)]
async fn a_block_during_a_sync_reports_the_block() {
    let error = sync_stopped_while_running(true).await;
    assert!(
        matches!(
            error,
            GroupError::Client(ClientError::BackendMismatch { .. })
        ),
        "the sync did not report the block: {error:?}"
    );
}

/// A close without a block still reports the cancelled barrier.
#[xmtp_common::test(unwrap_try = true)]
async fn a_close_during_a_sync_reports_a_cancelled_barrier() {
    let error = sync_stopped_while_running(false).await;
    let GroupError::Sync(summary) = &error else {
        panic!("a close without a block is not a blocked connection: {error:?}");
    };
    let cancelled = |error: &GroupError| {
        let barrier = match error {
            GroupError::StreamBarrier(barrier) => Some(barrier),
            GroupError::PublishedButUnconfirmed { cause, .. } => cause.as_deref(),
            _ => None,
        };
        matches!(
            barrier,
            Some(BarrierError::Incomplete {
                reason: BarrierFailure::Cancelled,
                ..
            })
        )
    };
    assert!(
        summary.other.as_deref().is_some_and(cancelled)
            || summary.post_commit_errors.iter().any(cancelled)
            || summary.publish_errors.iter().any(cancelled),
        "the close did not report a cancelled barrier: {error:?}"
    );
}
