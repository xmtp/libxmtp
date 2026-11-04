//! Welcome sync tests: held allowances and rate-limited retries.

use super::*;

/// A full held allowance gives up the oldest held Welcome, so new holds
/// cannot grow held storage without bound.
// verifies: PROC-020, JOIN-057
#[xmtp_common::test(unwrap_try = true)]
async fn a_full_held_allowance_rejects_the_oldest_held_welcome() {
    use xmtp_db::incoming_envelope::NewIncomingEnvelope;

    let bo = EngineTestPeer::new().await;
    let client = bo
        .with_stream_policy(|settings| settings.welcome_pending.rows = 2)
        .await;
    let service = WelcomeService::new(client.context.clone());
    let db = client.context.db();
    let limits = client
        .context
        .incoming_runtime()
        .policy()
        .incoming_limits(NetworkEntityKind::Welcome);
    // Three Welcomes for groups that require a later client version.
    for sequence in 1..=3 {
        let envelope = NewIncomingEnvelope {
            sequence_id: Cursor(sequence),
            envelope: vec![1],
        };
        db.admit_ordered_batch(&service.topic(), Cursor(sequence - 1), &[envelope], limits)?;
        let pending = db
            .pending_envelope(&service.topic(), Cursor(sequence))?
            .unwrap();
        let floor = GroupError::UnsupportedWelcomeVersion("999.0.0".into());
        service.classify_pending_result(&pending, Err(floor))?;
    }
    assert!(db.pending_envelope(&service.topic(), Cursor(1))?.is_none());
    let rejection = db.read_last_rejection(&service.topic())?.unwrap();
    assert_eq!(
        (rejection.sequence_id, rejection.code.as_str()),
        (Cursor(1), "held_capacity")
    );
    for kept in [2, 3] {
        assert!(
            db.pending_envelope(&service.topic(), Cursor(kept))?
                .unwrap()
                .blocked
        );
    }
}

/// A Welcome refused by a rate limit is not retried before the delay.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_rate_limited_welcome_retries_after_the_delay() {
    use xmtp_db::incoming_envelope::NewIncomingEnvelope;
    let bo = EngineTestPeer::new().await;
    let service = WelcomeService::new(bo.context.clone());
    let db = bo.context.db();
    db.admit_ordered_batch(
        &service.topic(),
        Cursor(0),
        &[NewIncomingEnvelope {
            sequence_id: Cursor(1),
            envelope: vec![1],
        }],
        bo.context
            .incoming_runtime()
            .policy()
            .incoming_limits(NetworkEntityKind::Welcome),
    )?;
    let pending = db.pending_envelope(&service.topic(), Cursor(1))?.unwrap();
    let delay = xmtp_common::time::Duration::from_secs(300);
    let refused = GroupError::WrappedApi(xmtp_api::ApiError::Api(
        xmtp_proto::api::NetworkError::new(xmtp_api_grpc::error::GrpcError::Status(
            xmtp_api_grpc::error::rate_limited_status("request", delay),
        )),
    ));
    let before = xmtp_common::time::now_ns();
    assert!(matches!(
        service.classify_pending_result(&pending, Err(refused))?,
        WelcomeHeadOutcome::Waiting { blocked: false, .. }
    ));
    let pending = db.pending_envelope(&service.topic(), Cursor(1))?.unwrap();
    assert!(pending.retry_at_ns >= before + delay.as_nanos() as i64);
}

/// A rate limit longer than a pointer's retention retries at the deadline,
/// where the pointer expires without a fetch, instead of never.
// verifies: RATE-041
#[xmtp_common::test(unwrap_try = true)]
async fn a_pointee_rate_limit_never_outlasts_its_retention() {
    use xmtp_db::incoming_envelope::NewIncomingEnvelope;
    let bo = EngineTestPeer::new().await;
    let service = WelcomeService::new(bo.context.clone());
    let db = bo.context.db();
    db.admit_ordered_batch(
        &service.topic(),
        Cursor(0),
        &[NewIncomingEnvelope {
            sequence_id: Cursor(1),
            envelope: vec![1],
        }],
        bo.context
            .incoming_runtime()
            .policy()
            .incoming_limits(NetworkEntityKind::Welcome),
    )?;
    let pending = db.pending_envelope(&service.topic(), Cursor(1))?.unwrap();
    let deadline = xmtp_common::time::now_ns() + xmtp_common::NS_IN_SEC;
    service.defer_pending(
        &pending,
        "welcome_pointee",
        false,
        Some(deadline),
        Some(xmtp_common::time::Duration::MAX),
    )?;
    let pending = db.pending_envelope(&service.topic(), Cursor(1))?.unwrap();
    assert!(pending.retry_at_ns <= deadline);
}
