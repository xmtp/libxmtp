//! A barrier-based sync as the first request after an offline build on
//! another deployment.
use super::*;
use crate::builder::ClientBuilder;
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

/// The deferred first-request check blocks the connection and cancels the
/// client's streams. A sync that waits on the processing barrier must still
/// report why, as a direct request does.
// verifies: CONF-077, CONF-064
#[xmtp_common::test(unwrap_try = true)]
async fn first_sync_all_on_another_deployment_reports_the_backend_mismatch() {
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

    let second = Client::builder(IdentityStrategy::CachedOnly)
        .store(TestDb::create_persistent_store(Some(path)).await)
        .with_scw_verifier(MockSmartContractSignatureVerifier::new(true))
        .default_mls_store()?
        .local()
        .with_allow_offline(Some(true))
        .with_disable_workers(true)
        .build()
        .await?;
    let error = second
        .sync_all_welcomes_and_groups(None)
        .await
        .expect_err("the first request must fail its deployment check");
    let crate::groups::GroupError::Sync(summary) = &error else {
        panic!("expected a sync summary, got {error:?}");
    };
    assert!(
        matches!(
            summary.other.as_deref(),
            Some(crate::groups::GroupError::StreamBarrier(
                crate::subscriptions::barrier::BarrierError::Incomplete {
                    reason: crate::subscriptions::barrier::BarrierFailure::Blocked,
                    ..
                }
            ))
        ),
        "a blocked connection is not a close by the app: {error:?}"
    );
    assert!(
        backend_mismatch_in_chain(&error),
        "the sync error lost the preflight cause: {error:?}"
    );
}
