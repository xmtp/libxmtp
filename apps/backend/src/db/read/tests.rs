use crate::test_support as support;

use crate::{Backend, api};
use crate::{
    api::identity_service_server::IdentityService, api::query_service_server::QueryService,
};
use support::{TestServer, query_topic};
use tonic::Request;
use xmtp_mls_validation::test_utils::inline_welcome_envelope;

#[xmtp_common::test(unwrap_try = true)]
async fn query_uses_primary_while_newest_and_lookup_use_read_pool() {
    let primary = TestServer::new(|_| {}).await?;
    let selected = TestServer::new(|_| {}).await?;
    let primary_meta = primary
        .publish(vec![inline_welcome_envelope([31; 32])])
        .await?
        .remove(0);
    let selected_meta = selected
        .publish(vec![inline_welcome_envelope([32; 32])])
        .await?
        .remove(0);

    let mut backend: Backend = primary.backend.clone();
    std::sync::Arc::make_mut(&mut backend.store).read = selected.backend.store.primary.clone();

    let query = QueryService::query(
        &backend,
        Request::new(api::QueryRequest {
            queries: vec![query_topic(primary_meta.topic.clone().unwrap(), 0)],
            limit: 1,
        }),
    )
    .await?
    .into_inner();
    assert_eq!(query.envelopes.len(), 1);
    assert_eq!(query.envelopes[0].meta, Some(primary_meta.clone()));

    let newest = QueryService::query_newest(
        &backend,
        Request::new(api::QueryNewestRequest {
            topics: vec![selected_meta.topic.clone().unwrap()],
            include_full_envelope: true,
        }),
    )
    .await?
    .into_inner();
    assert_eq!(newest.results.len(), 1);
    assert_eq!(newest.results[0].meta, Some(selected_meta.clone()));
    assert_eq!(
        newest.results[0].envelope,
        Some(inline_welcome_envelope([32; 32]))
    );

    sqlx::query("INSERT INTO identifier_association VALUES ('abcd', 2, $1, 1, NULL)")
        .bind(vec![9_u8; 32])
        .execute(&selected.backend.store.primary)
        .await?;
    let lookup = IdentityService::get_inbox_ids(
        &backend,
        Request::new(api::GetInboxIdsRequest {
            requests: vec![api::get_inbox_ids_request::Request {
                identifier: "abcd".into(),
                identifier_kind: 2,
            }],
        }),
    )
    .await?
    .into_inner();
    assert_eq!(lookup.responses.len(), 1);
    assert_eq!(lookup.responses[0].inbox_id, Some(hex::encode([9_u8; 32])));

    primary.stop().await?;
    selected.stop().await?;
}
