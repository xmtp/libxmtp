//! The leaves of `groups/mls_sync/publish/tests/deadlines.rs` that need the client.

use crate::groups::test_support::stalled_api::{StalledApi, StalledCall};
use crate::groups::{GroupError, MlsGroup};
use crate::prelude::*;
use crate::tester;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use xmtp_common::time::{Duration, timeout};
use xmtp_db::group::GroupQueryArgs;

type StalledContext = Arc<
    crate::context::XmtpMlsLocalContext<
        StalledApi,
        xmtp_db::DefaultStore,
        crate::utils::TestMlsStorage,
    >,
>;

/// A client on `tester`'s registered state whose backend client never
/// answers `call`, with `budget` as the intent wait deadline.
async fn client_with_stalled_api_within(
    tester: &crate::utils::ClientTester,
    call: StalledCall,
    budget: Duration,
) -> crate::Client<StalledContext> {
    let api = StalledApi {
        inner: tester.context.api().api_client.raw_for_test().clone(),
        call,
        entered: Arc::new(AtomicBool::new(false)),
        cancelled: Arc::new(AtomicBool::new(false)),
        group_messages: Arc::new(AtomicUsize::new(0)),
        welcomes: Arc::new(AtomicUsize::new(0)),
    };
    let mut settings = tester.context.incoming_runtime().policy().clone();
    settings.barrier_timeout = budget;
    // Reuse registered tester state to inject one non-completing async RPC.
    crate::builder::ClientBuilder::from_client(tester.client.clone())
        .api_client(api)
        .with_disable_workers(true)
        .with_allow_offline(Some(true))
        .stream_policy(settings)
        .build()
        .await
        .unwrap()
}

/// Find-or-create on an interrupted DM reports its own repair's outcome, as a
/// fresh creation reports its add: an unconfirmed add is not a returned DM.
// verifies: DMS-020
#[xmtp_common::test(unwrap_try = true)]
async fn find_or_create_dm_reports_its_unconfirmed_repair() {
    /// Long enough for the add's round on a loaded machine.
    const ADD_BUDGET: Duration = Duration::from_secs(5);
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    MlsGroup::create_dm_without_peer(&alix.context, bo.inbox_id().to_string())?;
    let client =
        client_with_stalled_api_within(&alix, StalledCall::WelcomePublish, ADD_BUDGET).await;

    let result = timeout(
        ADD_BUDGET * 3,
        client.find_or_create_dm(bo.inbox_id(), None),
    )
    .await?;
    assert!(
        matches!(
            &result,
            Err(crate::client::ClientError::Group(error))
                if matches!(**error, GroupError::PublishedButUnconfirmed { .. })
        ),
        "{:?}",
        result.map(|dm| dm.group_id)
    );
}

// verifies: GMOD-056
#[xmtp_common::test(unwrap_try = true)]
async fn a_create_with_an_unconfirmed_add_returns_the_group() {
    const ADD_BUDGET: Duration = Duration::from_secs(5);
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);
    let client =
        client_with_stalled_api_within(&alix, StalledCall::WelcomePublish, ADD_BUDGET).await;
    let result = timeout(
        ADD_BUDGET * 3,
        client.create_group_with_members(&[bo.inbox_id()], None, None),
    )
    .await?;
    assert!(
        result.is_ok(),
        "{:?}",
        result.as_ref().map(|group| group.group_id)
    );
    let group = result?;
    assert_eq!(alix.find_groups(GroupQueryArgs::default())?.len(), 1);
    assert!(
        client
            .context
            .api()
            .api_client
            .raw_for_test()
            .entered
            .load(Ordering::SeqCst)
    );
    alix.group(&group.group_id)?.sync().await?;
    bo.wait_for_welcomes().await?;
    assert_eq!(bo.group(&group.group_id)?.members().await?.len(), 2);
}
