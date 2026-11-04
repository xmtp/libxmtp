use xmtp_mls::tester;
use xmtp_mls_core::context::CoreContext;

#[xmtp_common::test(flavor = "multi_thread", worker_threads = 5, unwrap_try = true)]
async fn test_process_streamed_welcome_message() {
    use prost::Message;
    use xmtp_proto::types::{Cursor, Topic};
    tester!(alix);
    tester!(bo);
    let alix_group = alix.create_group(None, None)?;
    alix_group.add_members(&[bo.inbox_id()]).await?;
    let envelopes = alix
        .context
        .api()
        .query_all(
            [(
                Topic::new_welcome_message(bo.context.installation_id()),
                Cursor(0),
            )]
            .into(),
            bo.context.api().limits().max_query_limit as u32,
        )
        .await?;
    assert!(!envelopes.is_empty(), "Should have at least one welcome");
    let groups = bo
        .process_streamed_welcome_message(envelopes[0].encode_to_vec())
        .await?;
    assert_eq!(groups.len(), 1, "Should have exactly one group");
}
