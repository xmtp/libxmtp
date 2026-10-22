//! Publish signatures and the envelope bound at the publish service.

use super::*;
use xmtp_mls_validation::test_utils::{
    group_keys, keyed_group_message_envelope, sealed_commit_log_envelope,
};

/// The index and reason of a refused publish.
fn rejection(error: tonic::Status) -> support::TestResult<(Option<u32>, Reason)> {
    assert_eq!(error.code(), Code::InvalidArgument, "{error:?}");
    let status = tonic_types::pb::Status::decode(error.details())?;
    let detail = api::PublishError::decode(status.details[0].value.as_slice())?;
    Ok((detail.index, detail.reason()))
}

async fn stored_rows(server: &TestServer) -> support::TestResult<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM envelopes")
        .fetch_one(&server.backend.store.primary)
        .await?)
}

fn group_message(envelope: &mut api::ClientEnvelope) -> &mut api::GroupMessage {
    match envelope.payload.as_mut() {
        Some(api::client_envelope::Payload::GroupMessage(message)) => message,
        _ => unreachable!("the fixture is a group message"),
    }
}

fn sealed_entry(envelope: &mut api::ClientEnvelope) -> &mut api::SealedCommitLogEntry {
    match envelope.payload.as_mut() {
        Some(api::client_envelope::Payload::SealedCommitLogEntry(entry)) => entry,
        _ => unreachable!("the fixture is a sealed commit-log entry"),
    }
}

/// One request may carry 1000 envelopes. The bound counts every input,
/// duplicates included, before parsing, so 1001 copies of a stored envelope
/// are refused as too large, with no index, although none would be stored.
#[xmtp_common::test(unwrap_try = true)]
// verifies: API-223, API-281
async fn the_envelope_bound_counts_every_input_and_names_no_envelope() {
    let server = TestServer::new(|_| {}).await?;
    let envelopes: Vec<_> = (0..1000u16)
        .map(|id| {
            let mut key = [0; 32];
            key[..2].copy_from_slice(&id.to_be_bytes());
            inline_welcome_envelope(key)
        })
        .collect();
    assert_eq!(server.publish(envelopes.clone()).await?.len(), 1000);
    assert_eq!(stored_rows(&server).await?, 1000);

    let error = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: vec![envelopes[0].clone(); 1001],
        })
        .await
        .unwrap_err();
    assert_eq!(rejection(error)?, (None, Reason::TooLarge));
    assert_eq!(
        server
            .publish(vec![envelopes[0].clone(); 1000])
            .await?
            .len(),
        1000
    );
    assert_eq!(stored_rows(&server).await?, 1000);
    server.stop().await?;
}

/// Signed group messages and sealed entries are stored with the retention
/// their kind has always had, under their 33-byte topics.
#[xmtp_common::test(unwrap_try = true)]
// verifies: API-212, API-230, API-300, TOPIC-001
async fn signed_publishes_are_stored_with_the_retention_of_their_kind() {
    let server = TestServer::new(|_| {}).await?;
    let keys = group_keys(1);
    let envelopes = vec![
        keyed_group_message_envelope(&keys, GroupMessageKind::Application, [0xff]),
        keyed_group_message_envelope(&keys, GroupMessageKind::Commit, []),
        sealed_commit_log_envelope(&keys, 7),
    ];
    let metas = server.publish(envelopes.clone()).await?;
    let group_topic = xmtp_proto::types::TopicKind::GroupMessagesV1
        .create(keys.group_id())
        .to_vec();
    let commit_log_topic = xmtp_proto::types::TopicKind::CommitLogEntriesV1
        .create(keys.commit_log_id())
        .to_vec();
    let topics: Vec<_> = metas
        .iter()
        .map(|meta| meta.topic.as_ref().unwrap().topic.clone())
        .collect();
    assert_eq!(topics[0], group_topic);
    assert_eq!(topics[1], group_topic);
    assert_eq!(topics[2], commit_log_topic);
    let retention = server.backend.config.retention.group_message_seconds;
    assert_eq!(
        metas[0].expiry_ns - metas[0].server_ns,
        retention * xmtp_common::NS_IN_SEC as u64
    );
    assert!(!metas[0].is_commit_or_proposal);
    assert_eq!(metas[1].expiry_ns, 0);
    assert!(metas[1].is_commit_or_proposal);
    assert_eq!(metas[2].expiry_ns, 0);
    assert!(!metas[2].is_commit_or_proposal);
    // The commit is the newest envelope on the group topic.
    for (envelope, meta) in envelopes.into_iter().zip(&metas).take(3).skip(1) {
        let fetched = server
            .query()
            .query_newest(api::QueryNewestRequest {
                topics: vec![meta.topic.clone().unwrap()],
                include_full_envelope: true,
            })
            .await?
            .into_inner()
            .results
            .pop()?;
        assert_eq!(fetched.envelope, Some(envelope));
    }
    server.stop().await?;
}

/// A 16-byte legacy group id names no topic, and field 5 is reserved. A group
/// message on a 16-byte group id, signed or not, and an old client's
/// commit-log entry are refused as malformed payloads, and nothing is stored.
/// Before the cutover the backend stored all three.
#[xmtp_common::test(unwrap_try = true)]
// verifies: TOPIC-002, API-210, API-230
async fn legacy_group_ids_and_retired_commit_log_entries_are_refused() {
    use xmtp_mls_validation::test_utils::{
        group_message_envelope, retired_commit_log_envelope_bytes,
    };
    let server = TestServer::new(|_| {}).await?;
    let legacy = [10; 16];
    let unsigned = group_message_envelope(legacy, GroupMessageKind::Application, []);
    let mut signed = unsigned.clone();
    group_message(&mut signed).signature = vec![0x55; 64];
    let retired =
        api::ClientEnvelope::decode(retired_commit_log_envelope_bytes(legacy).as_slice())?;
    for (case, legacy_envelope) in [
        ("an unsigned message on a 16-byte group id", unsigned),
        (
            "a message with a 64-byte signature on a 16-byte group id",
            signed,
        ),
        ("a retired commit-log entry", retired),
    ] {
        let error = server
            .publisher()
            .publish(api::PublishRequest {
                envelopes: vec![legacy_envelope],
            })
            .await
            .unwrap_err();
        assert_eq!(
            rejection(error)?,
            (Some(0), Reason::MalformedPayload),
            "{case}"
        );
    }
    assert_eq!(stored_rows(&server).await?, 0);
    server.stop().await?;
}

/// Each refused case sits at index 1 behind a valid envelope. A wrong-length
/// signed field is a malformed payload; a signature that does not verify is
/// an invalid signature. Nothing is stored.
#[xmtp_common::test(unwrap_try = true)]
// verifies: API-223, API-230, API-281, API-300
async fn signed_publishes_are_refused_unless_their_signature_verifies() {
    use xmtp_mls_validation::test_utils::{
        group_message_envelope, retired_commit_log_envelope_bytes,
    };
    let server = TestServer::new(|_| {}).await?;
    let keys = group_keys(1);
    let message = keyed_group_message_envelope(&keys, GroupMessageKind::Application, []);
    let entry = sealed_commit_log_envelope(&keys, 7);
    let changed = |envelope: &api::ClientEnvelope, change: fn(&mut api::ClientEnvelope)| {
        let mut envelope = envelope.clone();
        change(&mut envelope);
        envelope
    };
    let mut identity_forgery = group_message_envelope(
        hex::decode(format!("01{}", "00".repeat(31)))?,
        GroupMessageKind::Application,
        [],
    );
    group_message(&mut identity_forgery).signature = hex::decode(format!(
        "{}01{}",
        "58".to_owned() + &"66".repeat(31),
        "00".repeat(31)
    ))?;
    let cases = [
        (
            "empty signature",
            changed(&message, |envelope| {
                group_message(envelope).signature.clear()
            }),
            Reason::MalformedPayload,
        ),
        (
            "63-byte signature",
            changed(&message, |envelope| {
                group_message(envelope).signature.pop();
            }),
            Reason::MalformedPayload,
        ),
        (
            "flipped should_push",
            changed(&message, |envelope| {
                let message = group_message(envelope);
                message.should_push = !message.should_push;
            }),
            Reason::InvalidSignature,
        ),
        (
            "changed sender_hmac",
            changed(&message, |envelope| {
                group_message(envelope).sender_hmac[0] ^= 1
            }),
            Reason::InvalidSignature,
        ),
        (
            "one appended data byte",
            changed(&message, |envelope| group_message(envelope).data.push(0)),
            Reason::InvalidSignature,
        ),
        (
            "another group's key",
            changed(&message, |envelope| {
                xmtp_mls_common::group_keys::sign_group_message(
                    &group_keys(2),
                    group_message(envelope),
                )
                .unwrap();
            }),
            Reason::InvalidSignature,
        ),
        (
            "a forgery under the identity point",
            identity_forgery,
            Reason::InvalidSignature,
        ),
        (
            "11-byte nonce",
            changed(&entry, |envelope| {
                sealed_entry(envelope).nonce.pop();
            }),
            Reason::MalformedPayload,
        ),
        (
            "15-byte ciphertext",
            changed(&entry, |envelope| {
                sealed_entry(envelope).ciphertext.truncate(15)
            }),
            Reason::MalformedPayload,
        ),
        (
            "flipped ciphertext byte",
            changed(&entry, |envelope| sealed_entry(envelope).ciphertext[0] ^= 1),
            Reason::InvalidSignature,
        ),
        (
            "a retired commit-log entry naming a 32-byte group id",
            api::ClientEnvelope::decode(
                retired_commit_log_envelope_bytes(keys.group_id()).as_slice(),
            )?,
            Reason::MalformedPayload,
        ),
    ];
    for (case, bad, reason) in cases {
        let error = server
            .publisher()
            .publish(api::PublishRequest {
                envelopes: vec![inline_welcome_envelope([60; 32]), bad],
            })
            .await
            .unwrap_err();
        assert_eq!(rejection(error)?, (Some(1), reason), "{case}");
        assert_eq!(stored_rows(&server).await?, 0, "{case}");
    }
    server.stop().await?;
}

/// Identical envelopes in one request collapse before validation, so a
/// signed envelope repeated 10 times is verified once.
#[xmtp_common::test(unwrap_try = true)]
// verifies: API-222
async fn a_repeated_signed_envelope_is_verified_once() {
    let server = TestServer::new(|_| {}).await?;
    let envelope = keyed_group_message_envelope(&group_keys(1), GroupMessageKind::Commit, []);
    let checks = &server.backend.signature_checks;
    checks.take();
    let metas = server.publish(vec![envelope; 10]).await?;
    assert_eq!(checks.take(), (1, 1), "one blocking task, one verification");
    assert!(metas.iter().all(|meta| *meta == metas[0]));
    assert_eq!(stored_rows(&server).await?, 1);
    server.stop().await?;
}

/// A replay of stored envelopes costs no verification and returns their
/// stored metadata; the first publish verified all of them in one task. Sent
/// with one new signed envelope, only the new one is verified, and a request
/// with no signed envelope starts no blocking task.
#[xmtp_common::test(unwrap_try = true)]
// verifies: API-222
async fn stored_signed_envelopes_are_not_verified_again() {
    let server = TestServer::new(|_| {}).await?;
    let mut envelopes = Vec::new();
    for seed in 0..50 {
        let keys = group_keys(seed);
        envelopes.push(keyed_group_message_envelope(
            &keys,
            GroupMessageKind::Application,
            [],
        ));
        envelopes.push(sealed_commit_log_envelope(&keys, 7));
    }
    let checks = &server.backend.signature_checks;
    checks.take();
    let stored = server.publish(envelopes.clone()).await?;
    assert_eq!(checks.take(), (1, 100));
    assert_eq!(server.publish(envelopes.clone()).await?, stored);
    assert_eq!(checks.take(), (0, 0));
    assert_eq!(stored_rows(&server).await?, 100);

    let mut mixed = envelopes;
    mixed.push(keyed_group_message_envelope(
        &group_keys(50),
        GroupMessageKind::Application,
        [],
    ));
    let metas = server.publish(mixed).await?;
    assert_eq!(checks.take(), (1, 1), "only the new envelope is verified");
    assert_eq!(metas[..100], stored[..]);
    assert_eq!(stored_rows(&server).await?, 101);

    server
        .publish(vec![inline_welcome_envelope([1; 32])])
        .await?;
    assert_eq!(checks.take(), (0, 0), "no signed envelope, no task");
    server.stop().await?;
}

/// A failed signature does not stop verification of the envelopes after it.
#[xmtp_common::test(unwrap_try = true)]
// verifies: API-223
async fn every_unchecked_signature_is_verified_without_an_early_stop() {
    let server = TestServer::new(|_| {}).await?;
    let keys = group_keys(1);
    let mut bad = sealed_commit_log_envelope(&keys, 7);
    sealed_entry(&mut bad).ciphertext[0] ^= 1;
    let mut also_bad = keyed_group_message_envelope(&keys, GroupMessageKind::Commit, []);
    let message = group_message(&mut also_bad);
    message.should_push = !message.should_push;
    let checks = &server.backend.signature_checks;
    checks.take();
    let error = server
        .publisher()
        .publish(api::PublishRequest {
            envelopes: vec![
                bad,
                sealed_commit_log_envelope(&keys, 8),
                also_bad,
                keyed_group_message_envelope(&keys, GroupMessageKind::Application, []),
            ],
        })
        .await
        .unwrap_err();
    assert_eq!(rejection(error)?, (Some(0), Reason::InvalidSignature));
    assert_eq!(checks.take(), (1, 4));
    assert_eq!(stored_rows(&server).await?, 0);
    server.stop().await?;
}
