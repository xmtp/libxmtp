use xmtp_common::toxiproxy_test;
use xmtp_db::group::GroupQueryArgs;

use crate::tester;

// verifies: GMOD-056
#[xmtp_common::test(unwrap_try = true)]
async fn test_bad_network() {
    toxiproxy_test(async || {
        tester!(alix, proxy);
        tester!(bo);

        // Cut the network connection and make a group with members
        alix.for_each_proxy(async |p| {
            p.disable().await.unwrap();
        })
        .await;
        let result = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await;

        // Resolution fails before the group is stored or a Welcome is sent.
        assert!(result.is_err());
        assert!(alix.find_groups(GroupQueryArgs::default())?.is_empty());
        bo.sync_welcomes().await?;
        assert!(bo.find_groups(GroupQueryArgs::default())?.is_empty());

        // Turn alix's connection back on.
        alix.for_each_proxy(async |p| p.enable().await.unwrap())
            .await;
        // Retry creation after connectivity returns.
        let g = alix
            .create_group_with_members(&[bo.inbox_id()], None, None)
            .await?;

        // Bo should get the welcome for the group.
        bo.sync_welcomes().await?;
        assert!(bo.group(&g.group_id).is_ok());
    })
    .await;
}
