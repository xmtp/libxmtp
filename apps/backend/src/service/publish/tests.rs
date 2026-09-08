use crate::test_support as support;

use crate::api::{self, publish_error::Reason};
use prost::Message;
use support::TestServer;
use tonic::Code;
use xmtp_mls_validation::test_utils::{
    GroupMessageKind, expired_key_package_envelope, group_message_envelope, identity_envelope,
    identity_history_with_passkey, inline_welcome_envelope,
    malformed_signature_create_inbox_update,
};

#[xmtp_common::test(unwrap_try = true)]
async fn application_envelope_limit_accepts_exact_size_and_rejects_one_past() {
    let envelope = group_message_envelope([1; 16], GroupMessageKind::Application, []);
    let size = envelope.encoded_len();
    let mut oversized = envelope.clone();
    if let Some(api::client_envelope::Payload::GroupMessage(message)) = &mut oversized.payload {
        message.data.push(1);
    }
    assert_eq!(oversized.encoded_len(), size + 1);

    let server = TestServer::new(|config| config.limits.max_envelope_bytes = size).await?;
    let exact = server.publish(vec![envelope]).await?;
    assert_eq!(exact.len(), 1);

    let error = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: vec![oversized],
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);
    let status = tonic_types::pb::Status::decode(error.details())?;
    let detail = api::PublishError::decode(status.details[0].value.as_slice())?;
    assert_eq!(detail.index, Some(0));
    assert_eq!(detail.reason(), Reason::TooLarge);
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn publish_topic_limit_accepts_exact_count_and_rejects_one_past() {
    let server = TestServer::new(|config| config.limits.max_publish_topics = 1).await?;
    let first = inline_welcome_envelope([2; 32]);
    let second = inline_welcome_envelope([3; 32]);
    let third = inline_welcome_envelope([4; 32]);
    assert_eq!(server.publish(vec![first]).await?.len(), 1);

    let error = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: vec![second, third],
        })
        .await
        .unwrap_err();
    assert_eq!(error.code(), Code::InvalidArgument);
    let status = tonic_types::pb::Status::decode(error.details())?;
    let detail = api::PublishError::decode(status.details[0].value.as_slice())?;
    assert_eq!(detail.index, None);
    assert_eq!(detail.reason(), Reason::TooLarge);
    server.stop().await?;
}

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
async fn publish_errors_keep_the_original_index_for_each_validation_reason() {
    let server = TestServer::new(|_| {}).await?;
    let fixture = identity_history_with_passkey().await;
    let cases = [
        (api::ClientEnvelope::default(), Reason::MalformedPayload),
        (
            expired_key_package_envelope().envelope,
            Reason::InvalidKeyPackage,
        ),
        (
            identity_envelope(fixture.update.clone()),
            Reason::InvalidIdentityUpdate,
        ),
        (
            identity_envelope(malformed_signature_create_inbox_update()),
            Reason::InvalidSignature,
        ),
    ];
    for (bad, reason) in cases {
        let good = inline_welcome_envelope([70; 32]);
        let error = server
            .publisher()
            .publish(api::PublishRequest {
                envelopes: vec![good.clone(), good, bad, api::ClientEnvelope::default()],
            })
            .await
            .unwrap_err();
        assert_eq!(error.code(), Code::InvalidArgument);
        let status = tonic_types::pb::Status::decode(error.details())?;
        let detail = api::PublishError::decode(status.details[0].value.as_slice())?;
        assert_eq!(detail.index, Some(2));
        assert_eq!(detail.reason(), reason);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM envelopes")
                .fetch_one(&server.backend.store.primary)
                .await?,
            0
        );
    }
    server.stop().await?;
}

#[xmtp_common::test(unwrap_try = true)]
async fn welcome_batch_commits_each_destination_and_canonical_payload() {
    let server = TestServer::new(|_| {}).await?;
    let envelopes: Vec<_> = (0..32)
        .map(|id| inline_welcome_envelope([id; 32]))
        .collect();
    let metas = server.publish(envelopes.clone()).await?;
    let watermarks: Vec<(Vec<u8>, i64)> =
        sqlx::query_as("SELECT topic, last_sequence_id FROM topic_watermark")
            .fetch_all(&server.backend.store.primary)
            .await?;
    assert_eq!(watermarks.len(), envelopes.len());
    assert_eq!(metas.len(), envelopes.len());
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
        assert!(watermarks.contains(&(
            meta.topic.unwrap().topic,
            meta.cursor.unwrap().sequence_id as i64,
        )));
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
    use xmtp_mls_validation::test_utils::{commit_log_envelope, group_message_envelope};
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
