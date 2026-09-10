use crate::test_support as support;

use crate::api;
use support::{TestServer, query_topic, topic};
use tonic::Code;
use xmtp_mls_validation::test_utils::inline_welcome_envelope;
use xmtp_proto::types::TopicKind;

#[xmtp_common::test(unwrap_try = true)]
async fn paging_coalesces_inputs_and_uses_one_total_clamped_limit() {
    let server = TestServer::new(|config| {
        config.limits.default_query_limit = 1;
        config.limits.max_query_limit = 2;
    })
    .await?;
    let first = inline_welcome_envelope([1; 32]);
    let mut second = first.clone();
    if let Some(api::client_envelope::Payload::WelcomeMessage(welcome)) = &mut second.payload
        && let Some(api::welcome_message::Version::V1(welcome)) = &mut welcome.version
    {
        welcome.data.push(1);
    }
    let other = inline_welcome_envelope([2; 32]);
    let metas = server.publish(vec![first, other, second]).await?;
    let topic_a = metas[0].topic.clone().unwrap();
    let topic_b = metas[1].topic.clone().unwrap();
    let request = api::QueryRequest {
        queries: vec![
            query_topic(topic_a.clone(), u64::MAX / 2),
            query_topic(topic_a.clone(), 0),
            query_topic(topic_b.clone(), u64::MAX / 2),
            api::TopicQuery {
                topic: Some(topic_b.clone()),
                cursor: None,
            },
        ],
        limit: u32::MAX,
    };
    let page = server.query().query(request).await?.into_inner();
    assert_eq!(page.envelopes.len(), 2);
    assert!(page.continuation.unwrap().has_more);
    assert_eq!(page.envelopes[0].meta, Some(metas[0].clone()));
    assert_eq!(page.envelopes[1].meta, Some(metas[1].clone()));
    let page = server
        .query()
        .query(api::QueryRequest {
            queries: vec![
                query_topic(topic_a, metas[0].cursor.as_ref().unwrap().sequence_id),
                query_topic(topic_b, metas[1].cursor.as_ref().unwrap().sequence_id),
            ],
            limit: 0,
        })
        .await?
        .into_inner();
    assert_eq!(page.envelopes.len(), 1);
    assert_eq!(page.envelopes[0].meta, Some(metas[2].clone()));
    assert!(!page.continuation.unwrap().has_more);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn newest_preserves_complete_metadata_without_payload_and_omits_empty_topics() {
    let server = TestServer::new(|_| {}).await?;
    let envelope = inline_welcome_envelope([3; 32]);
    let meta = server.publish(vec![envelope.clone()]).await?.remove(0);
    let topic = meta.topic.clone().unwrap();
    for full in [false, true] {
        let result = server
            .query()
            .query_newest(api::QueryNewestRequest {
                topics: vec![
                    topic.clone(),
                    topic.clone(),
                    support::topic(TopicKind::WelcomeMessagesV1, &[4; 32]),
                ],
                include_full_envelope: full,
            })
            .await?
            .into_inner();
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].meta, Some(meta.clone()));
        assert_eq!(result.results[0].envelope, full.then(|| envelope.clone()));
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn newest_limits_accept_exact_count_and_reject_one_past_for_both_modes() {
    let server = TestServer::new(|config| {
        config.limits.max_newest_metadata_topics = 1;
        config.limits.max_newest_full_topics = 1;
    })
    .await?;
    let envelope = inline_welcome_envelope([4; 32]);
    let meta = server.publish(vec![envelope.clone()]).await?.remove(0);
    let primary_topic = meta.topic.clone().unwrap();
    let other = topic(TopicKind::WelcomeMessagesV1, &[5; 32]);

    for include_full_envelope in [false, true] {
        let result = server
            .query()
            .query_newest(api::QueryNewestRequest {
                topics: vec![primary_topic.clone()],
                include_full_envelope,
            })
            .await?
            .into_inner();
        assert_eq!(result.results.len(), 1);
        assert_eq!(result.results[0].meta, Some(meta.clone()));
        assert_eq!(
            result.results[0].envelope,
            include_full_envelope.then(|| envelope.clone())
        );

        assert_eq!(
            server
                .query()
                .query_newest(api::QueryNewestRequest {
                    topics: vec![primary_topic.clone(), other.clone()],
                    include_full_envelope,
                })
                .await
                .unwrap_err()
                .code(),
            Code::InvalidArgument
        );
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn reads_reject_invalid_topics_cursors_and_original_item_counts() {
    let server = TestServer::new(|config| {
        config.limits.max_query_topics = 2;
        config.limits.max_newest_metadata_topics = 1;
        config.limits.max_newest_full_topics = 1;
    })
    .await?;
    let valid = topic(TopicKind::WelcomeMessagesV1, &[5; 32]);
    for queries in [
        vec![query_topic(valid.clone(), u64::MAX)],
        vec![query_topic(topic(TopicKind::KeyPackagesV1, &[6; 32]), 0)],
        vec![api::TopicQuery::default()],
        vec![query_topic(api::Topic { topic: vec![255] }, 0)],
        vec![
            query_topic(valid.clone(), 0),
            query_topic(valid.clone(), u64::MAX),
        ],
        vec![query_topic(valid.clone(), 0); 3],
    ] {
        assert_eq!(
            server
                .query()
                .query(api::QueryRequest { queries, limit: 1 })
                .await
                .unwrap_err()
                .code(),
            Code::InvalidArgument
        );
    }
    let empty = server
        .query()
        .query(api::QueryRequest {
            queries: vec![query_topic(valid.clone(), i64::MAX as u64)],
            limit: 1,
        })
        .await?
        .into_inner();
    assert!(empty.envelopes.is_empty());
    assert!(!empty.continuation.unwrap().has_more);
    assert_eq!(
        server
            .query()
            .query_newest(api::QueryNewestRequest {
                topics: vec![valid.clone(), valid],
                include_full_envelope: false
            })
            .await
            .unwrap_err()
            .code(),
        Code::InvalidArgument
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn full_query_and_newest_fail_when_response_bytes_exceed_the_cap() {
    let server = TestServer::new(|config| {
        config.limits.max_envelope_bytes = 1_024;
        config.limits.max_response_bytes = 1_500;
    })
    .await?;
    let mut envelopes = Vec::new();
    for id in 0..3 {
        let mut envelope = inline_welcome_envelope([id; 32]);
        if let Some(api::client_envelope::Payload::WelcomeMessage(welcome)) = &mut envelope.payload
            && let Some(api::welcome_message::Version::V1(version)) = &mut welcome.version
        {
            version.data = vec![3; 700];
        }
        envelopes.push(envelope);
    }
    let metas = server.publish(envelopes).await?;
    let topics: Vec<_> = metas
        .iter()
        .map(|meta| meta.topic.clone().unwrap())
        .collect();
    let queries = topics
        .iter()
        .cloned()
        .map(|topic| query_topic(topic, 0))
        .collect();
    assert_eq!(
        server
            .query()
            .query(api::QueryRequest { queries, limit: 3 })
            .await
            .unwrap_err()
            .code(),
        Code::OutOfRange
    );
    assert_eq!(
        server
            .query()
            .query_newest(api::QueryNewestRequest {
                topics: topics.clone(),
                include_full_envelope: true,
            })
            .await
            .unwrap_err()
            .code(),
        Code::OutOfRange
    );
    let metadata = server
        .query()
        .query_newest(api::QueryNewestRequest {
            topics,
            include_full_envelope: false,
        })
        .await?
        .into_inner();
    assert_eq!(metadata.results.len(), 3);
    server.stop().await?;
}
