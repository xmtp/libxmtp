//! Group additions and commit-log writes under a deployment's published
//! configuration, applied through the static provider.

use crate::context::CoreContext;
use crate::groups::GroupError;
use std::sync::Arc;
use xmtp_configuration::{ConfigProvider, ServerConfiguration, StaticConfigProvider};
use xmtp_db::group::GroupQueryArgs;
use xmtp_db::prelude::*;

fn provider(edit: impl FnOnce(&mut ServerConfiguration)) -> Arc<dyn ConfigProvider> {
    Arc::new(StaticConfigProvider::edited(edit))
}

// The deployment's ceiling is checked before the commit is built and
// before anything is published.
// verifies: CONF-043, GMOD-056
#[xmtp_common::test(unwrap_try = true)]
async fn a_lowered_group_member_limit_refuses_the_addition() {
    crate::tester!(bo);
    crate::tester!(caro);
    let invitees = [bo.identifier(), caro.identifier()];

    // A deployment that publishes room for one.
    crate::tester!(alix, config_provider: provider(|c| c.mls.max_group_members = 1));
    let group = alix.create_group(None, None)?;
    let error = group.add_members_by_identity(&invitees).await.unwrap_err();
    assert!(
        matches!(error, GroupError::UserLimitExceeded),
        "expected the user-limit error, got {error}"
    );

    // The commit was never built, so nobody joined.
    assert!(
        group.members().await?.len() < 2,
        "a refused addition must not reach the commit"
    );

    // Room for two counts the creator, who sits at sequence id zero until the
    // first commit and so is invisible to `members()`. Two invitees would make
    // three, and are refused; one fits exactly.
    crate::tester!(dana, config_provider: provider(|c| c.mls.max_group_members = 2));
    let error = dana
        .create_group_with_members(&[bo.inbox_id(), caro.inbox_id()], None, None)
        .await
        .map(|_| ())
        .expect_err("creation must respect the ceiling");
    assert!(matches!(error, crate::client::ClientError::Group(error)
        if matches!(*error, GroupError::UserLimitExceeded)));
    assert!(dana.find_groups(GroupQueryArgs::default())?.is_empty());
    let group = dana.create_group(None, None)?;
    let error = group.add_members_by_identity(&invitees).await.unwrap_err();
    assert!(
        matches!(error, GroupError::UserLimitExceeded),
        "the creator must count against the ceiling, got {error}"
    );
    group.add_members_by_identity(&invitees[..1]).await?;
    assert_eq!(group.members().await?.len(), 2);

    // The inbox-id API enforces the same ceiling: it is the method both entry
    // points reach, so it cannot be used to step over the limit.
    let error = group
        .add_members(&[caro.inbox_id()])
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("limit"),
        "the direct inbox-id API must enforce the ceiling too, got {error}"
    );

    // A deployment with room for three admits both.
    crate::tester!(eve, config_provider: provider(|c| c.mls.max_group_members = 3));
    let group = eve.create_group(None, None)?;
    group.add_members_by_identity(&invitees).await?;
    assert_eq!(group.members().await?.len(), 3);
}

// A deployment that keeps no commit log gets no commit-log entries,
// whatever the client's own worker switch says.
// verifies: CONF-045
#[xmtp_common::test(unwrap_try = true)]
async fn a_deployment_that_keeps_no_commit_log_writes_none() {
    crate::tester!(alix, config_provider: provider(|c| c.mls.commit_log_enabled = Some(false)));
    crate::tester!(bo);
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
    crate::tester!(cassie, config_provider: provider(|c| c.mls.commit_log_enabled = Some(true)));
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
