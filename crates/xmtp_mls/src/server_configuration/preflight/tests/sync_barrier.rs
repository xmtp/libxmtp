//! Barrier-based syncs when the connection is blocked while they run.
use super::*;
use crate::builder::ClientBuilder;
use crate::groups::GroupError;
use crate::server_configuration::BlockedConnection;
use crate::subscriptions::barrier::{BarrierError, BarrierFailure};
use crate::tester;
use crate::utils::test::register_client;
use xmtp_cryptography::utils::generate_local_wallet;

fn backend_mismatch_in_chain(error: &(dyn std::error::Error + 'static)) -> bool {
    let mut next = Some(error);
    while let Some(error) = next {
        if matches!(
            error.downcast_ref::<ClientError>(),
            Some(ClientError::BackendMismatch { .. })
        ) {
            return true;
        }
        next = error.source();
    }
    false
}

/// A client created online, whose stored configuration then names another
/// deployment, built again offline. Its first request fails the deferred
/// deployment check, which blocks the connection and cancels the client.
async fn offline_client_on_another_deployment() -> Result<TestClient2, Box<dyn std::error::Error>> {
    let owner = generate_local_wallet();
    let path = xmtp_common::tmp_path();
    let first = ClientBuilder::new_test_builder(&owner)
        .await
        .store(TestDb::create_persistent_store(Some(path.clone())).await)
        .with_disable_workers(true)
        .build()
        .await?;
    register_client(&first, &owner).await;
    let stored = first.context.db().server_configuration()?.unwrap();
    first.context.db().store_server_configuration(
        "org.example.other-deployment",
        "http://moved.example",
        &stored.response,
        stored.fetched_at_ns,
    )?;
    first.close().await?;
    drop(first);

    Ok(Client::builder(IdentityStrategy::CachedOnly)
        .store(TestDb::create_persistent_store(Some(path)).await)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .default_mls_store()?
        .local()
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?)
}

type TestClient2 = crate::utils::FullXmtpClient;

/// A sync that waits on the processing barrier reports the failed deployment
/// check, as a direct request does, not the barrier it cut off.
// verifies: CONF-077
#[xmtp_common::test(unwrap_try = true)]
async fn first_sync_all_on_another_deployment_reports_the_backend_mismatch() {
    let client = offline_client_on_another_deployment().await?;
    let error = client
        .sync_all_welcomes_and_groups(None)
        .await
        .expect_err("the first request must fail its deployment check");
    assert!(
        matches!(
            error,
            GroupError::Client(ClientError::BackendMismatch { .. })
        ),
        "the sync did not report the deployment check: {error:?}"
    );
}

/// Catch-up waits on the same barriers and reports the same failure.
// verifies: CONF-077
#[xmtp_common::test(unwrap_try = true)]
async fn first_catch_up_on_another_deployment_reports_the_backend_mismatch() {
    let client = offline_client_on_another_deployment().await?;
    let error = client
        .catch_up_to_live(None)
        .await
        .expect_err("the first request must fail its deployment check");
    assert!(
        backend_mismatch_in_chain(&error),
        "catch-up did not report the deployment check: {error:?}"
    );
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
