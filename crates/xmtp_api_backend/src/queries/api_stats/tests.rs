use super::*;
use crate::test::MockBackendClient;
use std::sync::Arc;
use xmtp_proto::{api::ApiClientError, api_client::XmtpMlsBidiStreams};

#[xmtp_common::async_trait]
impl XmtpMlsBidiStreams for MockBackendClient {
    type SubscribeStream = futures::stream::Empty<Result<SubscribeResponse, ApiClientError>>;
    type Error = ApiClientError;
    fn host(&self) -> &str {
        "mock://stats"
    }
    async fn subscribe_bidi(
        &self,
        _: futures::stream::BoxStream<'static, SubscribeRequest>,
    ) -> Result<Self::SubscribeStream, Self::Error> {
        Ok(futures::stream::empty())
    }
}

fn vector(client: &impl HasStats) -> [usize; 7] {
    let api = client.mls_stats();
    let identity = client.identity_stats();
    [
        api.publish.get_count(),
        api.query.get_count(),
        api.query_newest.get_count(),
        api.subscribe.get_count(),
        api.subscribe_static.get_count(),
        identity.get_inbox_ids.get_count(),
        identity.verify_smart_contract_wallet_signatures.get_count(),
    ]
}

// verifies: API-292, API-293, API-295
#[xmtp_common::test(unwrap_try = true)]
async fn seven_rpc_counters_share_and_clear_without_changing_isolated_clients() {
    let mut mock = MockBackendClient::new();
    mock.expect_publish()
        .times(1)
        .returning(|_| Ok(PublishResponse::default()));
    mock.expect_query()
        .times(2)
        .returning(|_| Ok(QueryResponse::default()));
    mock.expect_query_newest()
        .times(3)
        .returning(|_| Ok(QueryNewestResponse::default()));
    mock.expect_subscribe_group_messages()
        .times(5)
        .returning(|_| Ok(Box::pin(futures::stream::empty())));
    mock.expect_get_inbox_ids()
        .times(6)
        .returning(|_| Ok(GetInboxIdsResponse::default()));
    mock.expect_verify_smart_contract_wallet_signatures()
        .times(7)
        .returning(|_| Ok(VerifySmartContractWalletSignaturesResponse::default()));
    let client = TrackedStatsClient::new(Arc::new(mock));
    let shared = client.clone();
    let mut isolated_mock = MockBackendClient::new();
    isolated_mock
        .expect_get_inbox_ids()
        .times(11)
        .returning(|_| Ok(GetInboxIdsResponse::default()));
    let isolated = TrackedStatsClient::new(Arc::new(isolated_mock));
    for _ in 0..11 {
        isolated
            .get_inbox_ids(GetInboxIdsRequest::default())
            .await?;
    }
    client.publish(PublishRequest::default()).await?;
    for _ in 0..2 {
        shared.query(QueryRequest::default()).await?;
    }
    for _ in 0..3 {
        client.query_newest(QueryNewestRequest::default()).await?;
    }
    for _ in 0..4 {
        let _ = shared
            .subscribe_bidi(Box::pin(futures::stream::empty()))
            .await?;
    }
    for _ in 0..5 {
        let _ = client.subscribe_group_messages(&[]).await?;
    }
    for _ in 0..6 {
        shared.get_inbox_ids(GetInboxIdsRequest::default()).await?;
    }
    for _ in 0..7 {
        client
            .verify_smart_contract_wallet_signatures(
                VerifySmartContractWalletSignaturesRequest::default(),
            )
            .await?;
    }
    assert_eq!(vector(&client), [1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(vector(&shared), vector(&client));
    assert_eq!(vector(&isolated), [0, 0, 0, 0, 0, 11, 0]);
    let aggregate = shared.aggregate_stats();
    assert_eq!(aggregate.mls.query_newest.get_count(), 3);
    assert_eq!(
        aggregate
            .identity
            .verify_smart_contract_wallet_signatures
            .get_count(),
        7
    );
    shared.mls_stats().clear();
    shared.identity_stats().clear();
    assert_eq!(vector(&client), [0; 7]);
    assert_eq!(vector(&shared), [0; 7]);
    assert_eq!(vector(&isolated), [0, 0, 0, 0, 0, 11, 0]);
}
