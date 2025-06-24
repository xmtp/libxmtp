use crate::tester;

#[xmtp_common::test(unwrap_try = true)]
async fn test_bad_network() {
    tester!(alix);
    tester!(bo);

    // alix.proxy().disable().await?;
    // alix.create_group_with_inbox_ids(&[bo.inbox_id()], None, None)
    // .await;
}
