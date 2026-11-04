//! The leaves of `groups/tests/test_configuration.rs` that need the client.

use crate::groups::GroupError;
use std::sync::Arc;
use xmtp_configuration::{ConfigProvider, ServerConfiguration, StaticConfigProvider};
use xmtp_db::group::GroupQueryArgs;

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
