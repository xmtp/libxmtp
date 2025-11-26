use xmtp_common::toxiproxy_test;
use xmtp_db::group::GroupQueryArgs;

use crate::tester;

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
            .create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
            .await;

        // The group should be created, but an error should be reported from trying to add members
        // without a network connection.
        assert!(result.is_err());

        // The group should still be created, even though the add members request didn't go through.
        let g = alix.find_groups(GroupQueryArgs::default())?.pop().unwrap();

        // Bo should not have received the welcome for the group.
        bo.sync_welcomes().await?;
        assert!(bo.group(&g.group_id).is_err());

        // Turn alix's connection back on.
        alix.for_each_proxy(async |p| p.enable().await.unwrap())
            .await;
        // Try adding bo again.
        g.add_members_by_inbox_id(&[bo.inbox_id()]).await?;

        // Bo should get the welcome for the group.
        bo.sync_welcomes().await?;
        assert!(bo.group(&g.group_id).is_ok());
    })
    .await;
}
