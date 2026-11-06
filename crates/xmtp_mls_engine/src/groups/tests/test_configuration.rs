//! Group additions and commit-log writes under a deployment's published
//! configuration, applied through the static provider.

use crate::groups::test_support::{EnginePeer, EngineSuiteOps, EngineTestPeer};
use std::sync::Arc;
use xmtp_configuration::{ConfigProvider, ServerConfiguration, StaticConfigProvider};
use xmtp_db::prelude::*;
use xmtp_mls_core::context::CoreContext;

fn provider(edit: impl FnOnce(&mut ServerConfiguration)) -> Arc<dyn ConfigProvider> {
    Arc::new(StaticConfigProvider::edited(edit))
}

// A deployment that keeps no commit log gets no commit-log entries,
// whatever the client's own worker switch says.
// verifies: CONF-045
#[xmtp_common::test(unwrap_try = true)]
async fn a_deployment_that_keeps_no_commit_log_writes_none() {
    let alix = EngineTestPeer::new_with(|o| {
        o.config_provider = Some(provider(|c| c.mls.commit_log_enabled = Some(false)))
    })
    .await;
    let bo = EngineTestPeer::new().await;
    let group = alix.create_group(None, None)?;
    group.add_members_by_identity(&[bo.identifier()]).await?;
    group.sync().await?;
    assert!(
        alix.context
            .db()
            .get_group_logs(&group.group_id)?
            .is_empty(),
        "a deployment with the commit log off must write no entries"
    );

    // The same work against a deployment that keeps one does write entries, so
    // the assertion above is about the snapshot and not about the test setup.
    let cassie = EngineTestPeer::new_with(|o| {
        o.config_provider = Some(provider(|c| c.mls.commit_log_enabled = Some(true)))
    })
    .await;
    let group = cassie.create_group(None, None)?;
    group.add_members_by_identity(&[bo.identifier()]).await?;
    group.sync().await?;
    assert!(
        !cassie
            .context
            .db()
            .get_group_logs(&group.group_id)?
            .is_empty(),
        "a deployment with the commit log on must write entries"
    );
}
