//! Welcome sync tests: pointer query failures and their retention.

use super::*;

/// How the backend fails a pointee query.
#[derive(Clone, Copy, Debug)]
enum PointeeQueryFailure {
    Unavailable,
    Denied,
    /// A row whose metadata the ordered read rejects. That is a backend
    /// fault, not a property of the pointee, unlike a first row whose
    /// payload does not decode.
    BadMetadata,
}

/// Query failures, including a response with invalid metadata, obey the
/// same fixed fetch window as absent pointees, including rows persisted as
/// welcome_retry by older clients. An existing unreadable hold keeps its
/// own deadline through transport failures. A metadata fault is held like
/// `Denied` once it is classified, so only a fresh row needs that case.
// verifies: JOIN-094, JOIN-087, JOIN-063, JOIN-035
#[rstest]
#[case::fresh_unavailable(None, false, PointeeQueryFailure::Unavailable)]
#[case::fresh_denied(None, false, PointeeQueryFailure::Denied)]
#[case::fresh_bad_metadata(None, false, PointeeQueryFailure::BadMetadata)]
#[case::legacy_retry_unavailable(Some("welcome_retry"), false, PointeeQueryFailure::Unavailable)]
#[case::legacy_retry_denied(Some("welcome_retry"), false, PointeeQueryFailure::Denied)]
#[case::unreadable_unavailable(Some("welcome_blocked"), true, PointeeQueryFailure::Unavailable)]
#[case::unreadable_denied(Some("welcome_blocked"), true, PointeeQueryFailure::Denied)]
#[xmtp_common::test(unwrap_try = true)]
async fn pointer_query_errors_have_bounded_retention(
    #[case] prior_code: Option<&'static str>,
    #[case] unreadable: bool,
    #[case] failure: PointeeQueryFailure,
) {
    use xmtp_proto::backend_v1 as wire;
    let retryable = matches!(failure, PointeeQueryFailure::Unavailable);
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use xmtp_db::{ConnectionExt, schema::incoming_envelopes};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let _bo2 = bo.new_installation().await;
    let _bo3 = bo.new_installation().await;
    let group = alix.create_group(None, None).unwrap();
    group.invite(&bo).await.unwrap();
    let welcome = bo
        .context
        .api()
        .query_welcome_messages(bo.context.installation_id())
        .await
        .unwrap()
        .pop()
        .unwrap();
    assert!(matches!(
        welcome.variant,
        xmtp_proto::types::WelcomeMessageType::WelcomePointer(_)
    ));
    pending_welcome_for_test(&bo.context, &welcome)
        .await
        .unwrap();
    let service = WelcomeService::new(bo.context.clone());
    let db = bo.context.db();
    let row = || {
        incoming_envelopes::table.find((
            bo.context.installation_id().to_vec(),
            EntityKind::Welcome,
            welcome.cursor.0 as i64,
        ))
    };
    let unreadable_deadline =
        unreadable.then(|| xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY);
    db.raw_query(|conn| {
        diesel::update(row())
            .set((
                incoming_envelopes::error_code.eq(prior_code),
                incoming_envelopes::blocked.eq(unreadable),
                incoming_envelopes::retry_expires_at_ns.eq(unreadable.then_some(1i64)),
                incoming_envelopes::unreadable_expires_at_ns.eq(unreadable_deadline),
            ))
            .execute(conn)
    })
    .unwrap();

    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_query_raw_envelopes().returning(move |request| {
        let status = match failure {
            PointeeQueryFailure::Unavailable => {
                tonic::Status::unavailable("pointee query unavailable")
            }
            PointeeQueryFailure::Denied => tonic::Status::permission_denied("pointee query denied"),
            PointeeQueryFailure::BadMetadata => {
                // The hash is one byte short of SHA-256.
                return Ok(wire::QueryResponse {
                    envelopes: vec![wire::ServerEnvelope {
                        meta: Some(wire::EnvelopeMeta {
                            topic: request.queries[0].topic.clone(),
                            cursor: Some(wire::Cursor { sequence_id: 1 }),
                            message_hash: Some(wire::MessageHash {
                                hash: Some(wire::message_hash::Hash::Sha256(vec![7; 31])),
                            }),
                            ..Default::default()
                        }),
                        envelope: None,
                    }],
                    continuation: Some(wire::Continuation { has_more: false }),
                }
                .into());
            }
        };
        Err(xmtp_proto::api::ApiClientError::client(
            xmtp_api_grpc::error::GrpcError::Status(status),
        ))
    });
    let failing_client: EngineTestPeer<_> = bo
        .with_api(std::sync::Arc::new(api), None, |options| {
            options.config_provider = Some(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::default(),
            ))
        })
        .await;
    let failing_service = WelcomeService::new(failing_client.context.clone());
    let blocked = unreadable || !retryable;
    assert!(matches!(
        failing_service.resolve_pending_welcome(welcome.cursor).await.unwrap(),
        WelcomeHeadOutcome::Waiting { blocked: actual, .. } if actual == blocked
    ));
    let pending = db
        .pending_envelope(&service.topic(), welcome.cursor)
        .unwrap()
        .unwrap();
    assert_eq!(
        pending.error_code.as_deref(),
        Some(if blocked {
            "welcome_blocked"
        } else {
            "welcome_pointee"
        })
    );
    assert_eq!(
        pending.retry_expires_at_ns,
        Some(if unreadable {
            1
        } else {
            welcome.timestamp() + WELCOME_RETENTION_NS
        })
    );
    assert_eq!(pending.unreadable_expires_at_ns, unreadable_deadline);
    assert!(db.read_last_rejection(&service.topic()).unwrap().is_none());

    db.raw_query(|conn| {
        diesel::update(row())
            .set((
                incoming_envelopes::retry_expires_at_ns.eq(Some(1i64)),
                incoming_envelopes::unreadable_expires_at_ns.eq(unreadable.then_some(1i64)),
            ))
            .execute(conn)
    })
    .unwrap();
    let failing_service = WelcomeService::new(failing_client.context.clone());
    assert!(matches!(
        failing_service
            .resolve_pending_welcome(welcome.cursor)
            .await
            .unwrap(),
        WelcomeHeadOutcome::Progress { result: Err(_), .. }
    ));
    assert!(!db.has_pending_welcomes().unwrap());
    assert!(db.find_group(&group.group_id).unwrap().is_none());
    assert_eq!(
        db.read_last_rejection(&service.topic())
            .unwrap()
            .unwrap()
            .code,
        if unreadable {
            "unsupported_welcome_expired"
        } else {
            "welcome_pointer_expired"
        }
    );
}
