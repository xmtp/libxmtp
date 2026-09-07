mod support;

use prost::Message;
use support::TestServer;
use tonic::Code;
use xmtp_backend::api::{self, publish_error::Reason};
use xmtp_mls_validation::test_utils::{expired_key_package_envelope, inline_welcome_envelope};

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_retries_preserve_original_metadata_and_positions() {
    let server = TestServer::new(|_| {}).await?;
    let first = inline_welcome_envelope([1; 32]);
    let second = inline_welcome_envelope([2; 32]);
    let batch = vec![first.clone(), second.clone(), first.clone()];
    let (left, right) = tokio::join!(server.publish(batch.clone()), server.publish(batch));
    let left = left?;
    assert_eq!(left, right?);
    assert_eq!(left[0], left[2]);
    assert!(
        left[0].cursor.as_ref().unwrap().sequence_id < left[1].cursor.as_ref().unwrap().sequence_id
    );
    assert_eq!(
        server.publish(vec![second, first]).await?,
        vec![left[1].clone(), left[0].clone()]
    );
    let count = sqlx::query_scalar!("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, Some(2));
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn failed_batch_leaves_new_topics_empty_and_reports_first_original_error() {
    let server = TestServer::new(|_| {}).await?;
    let retained = inline_welcome_envelope([3; 32]);
    server.publish(vec![retained.clone()]).await?;
    let request = api::PublishRequest {
        envelopes: vec![
            retained,
            expired_key_package_envelope().envelope,
            api::ClientEnvelope::default(),
            inline_welcome_envelope([4; 32]),
        ],
    };
    let error = server.publisher().publish(request).await.unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);
    let status = tonic_types::pb::Status::decode(error.details())?;
    let detail = api::PublishError::decode(status.details[0].value.as_slice())?;
    assert_eq!(detail.index, Some(1));
    assert_eq!(detail.reason(), Reason::InvalidKeyPackage);
    let count = sqlx::query_scalar!("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(count, Some(1));
    let watermarks = sqlx::query_scalar!("SELECT count(*) FROM topic_watermark")
        .fetch_one(&server.backend.store.primary)
        .await?;
    assert_eq!(watermarks, Some(1));
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn welcome_batch_commits_each_destination_and_canonical_payload() {
    let server = TestServer::new(|_| {}).await?;
    let envelopes: Vec<_> = (0..32)
        .map(|id| inline_welcome_envelope([id; 32]))
        .collect();
    let metas = server.publish(envelopes.clone()).await?;
    for (envelope, meta) in envelopes.into_iter().zip(metas) {
        let fetched = server
            .query()
            .get(api::GetRequest {
                sequence_id: meta.cursor.as_ref().unwrap().sequence_id,
            })
            .await?
            .into_inner();
        assert_eq!(fetched.envelope, Some(envelope.clone()));
        assert_eq!(fetched.meta, Some(meta.clone()));
        assert_eq!(meta.expiry_ns - meta.server_ns, 7_776_000_000_000_000);
        let hash = xmtp_proto::types::canonical_envelope(&envelope).hash;
        assert_eq!(
            meta.message_hash.unwrap().hash,
            Some(api::message_hash::Hash::Sha256(hash.to_vec()))
        );
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn watermark_guard_failure_rolls_back_every_new_envelope() {
    let server = TestServer::new(|_| {}).await?;
    let topic = xmtp_proto::types::TopicKind::WelcomeMessagesV1
        .create([7; 32])
        .to_vec();
    sqlx::query!(
        "INSERT INTO topic_watermark (topic, last_sequence_id) VALUES ($1, 9223372036854775807)",
        &topic
    )
    .execute(&server.backend.store.primary)
    .await?;
    let result = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: vec![
                inline_welcome_envelope([7; 32]),
                inline_welcome_envelope([8; 32]),
            ],
        })
        .await;
    assert_eq!(result.unwrap_err().code(), Code::Internal);
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM envelopes")
            .fetch_one(&server.backend.store.primary)
            .await?,
        Some(0)
    );
    assert_eq!(
        sqlx::query_scalar!("SELECT count(*) FROM topic_watermark")
            .fetch_one(&server.backend.store.primary)
            .await?,
        Some(1)
    );
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn permanent_payloads_and_expired_metadata_remain_readable() {
    use xmtp_mls_validation::test_utils::{
        GroupMessageKind, commit_log_envelope, group_message_envelope,
    };
    let server = TestServer::new(|_| {}).await?;
    let envelopes = vec![
        group_message_envelope([10; 16], GroupMessageKind::Application, [0xff]),
        group_message_envelope([10; 16], GroupMessageKind::Proposal, []),
        group_message_envelope([10; 16], GroupMessageKind::Commit, []),
        commit_log_envelope([10; 16]),
    ];
    let metas = server.publish(envelopes.clone()).await?;
    assert!(metas[0].expiry_ns > metas[0].server_ns);
    for meta in &metas[1..] {
        assert_eq!(meta.expiry_ns, 0);
    }
    assert!(!metas[0].is_commit_or_proposal);
    assert!(metas[1].is_commit_or_proposal);
    assert!(metas[2].is_commit_or_proposal);
    assert!(!metas[3].is_commit_or_proposal);
    let id = metas[0].cursor.as_ref().unwrap().sequence_id as i64;
    sqlx::query!(
        "UPDATE envelopes SET server_ns = 0, expiry_ns = 1 WHERE sequence_id = $1",
        id
    )
    .execute(&server.backend.store.primary)
    .await?;
    let fetched = server
        .query()
        .get(api::GetRequest {
            sequence_id: id as u64,
        })
        .await?
        .into_inner();
    assert_eq!(fetched.envelope, Some(envelopes[0].clone()));
    assert_eq!(fetched.meta.unwrap().expiry_ns, 1);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn valid_key_packages_are_retained_and_served_by_newest() {
    use xmtp_mls_validation::test_utils::key_package_envelope;
    let server = TestServer::new(|_| {}).await?;
    let fixture = key_package_envelope("", Default::default());
    let original = server.publish(vec![fixture.envelope.clone()]).await?;
    assert_eq!(
        server.publish(vec![fixture.envelope.clone()]).await?,
        original
    );
    let newest = server
        .query()
        .query_newest(api::QueryNewestRequest {
            topics: vec![original[0].topic.clone().unwrap()],
            include_full_envelope: true,
        })
        .await?
        .into_inner();
    assert_eq!(newest.results[0].envelope, Some(fixture.envelope));
    assert_eq!(newest.results[0].meta, Some(original[0].clone()));
    server.stop().await?;
}
