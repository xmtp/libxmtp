//! Welcome sync tests: expired unsupported welcomes and legacy blocked pointers.

use super::*;

/// Gaining format support or restoring a missing key package must permit
/// the final attempt to join, even after the unreadable deadline expires.
// verifies: JOIN-087, JOIN-084
#[rstest]
#[case::wrapper("unsupported_welcome")]
#[case::missing_key("welcome_blocked")]
#[xmtp_common::test(unwrap_try = true)]
async fn an_expired_unsupported_welcome_still_installs_once_it_is_supported(
    #[case] code: &'static str,
    #[values(false, true)] pointer: bool,
) {
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use openmls_traits::storage::StorageProvider;
    use xmtp_db::ConnectionExt;
    use xmtp_db::schema::incoming_envelopes;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let _other_installations = if pointer {
        let bo2 = bo.new_installation().await;
        let bo3 = bo.new_installation().await;
        Some((bo2, bo3))
    } else {
        None
    };
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
    assert_eq!(
        matches!(
            welcome.variant,
            xmtp_proto::types::WelcomeMessageType::WelcomePointer(_)
        ),
        pointer
    );
    let cursor = welcome.cursor;
    pending_welcome_for_test(&bo.context, &welcome)
        .await
        .unwrap();
    let service = WelcomeService::new(bo.context.clone());
    let db = bo.context.db();

    let missing_key = if code == "welcome_blocked" {
        let history = db
            .find_key_package_history_entries_before_id(i32::MAX)
            .unwrap()
            .pop()
            .unwrap();
        let hash_ref = xmtp_mls_core::identity::deserialize_key_package_hash_ref(
            &history.key_package_hash_ref,
        )
        .unwrap();
        let storage = bo.context.mls_storage();
        let bundle: openmls::prelude::KeyPackageBundle =
            storage.key_package(&hash_ref).unwrap().unwrap();
        storage.delete_key_package(&hash_ref).unwrap();
        assert!(matches!(
            service.resolve_pending_welcome(cursor).await.unwrap(),
            WelcomeHeadOutcome::Waiting { blocked: true, .. }
        ));
        assert!(
            db.pending_envelope(&service.topic(), cursor)
                .unwrap()
                .unwrap()
                .unreadable_expires_at_ns
                .unwrap()
                > xmtp_common::time::now_ns()
        );
        Some((hash_ref, bundle))
    } else {
        None
    };

    // Simulate the first retry after the unreadable deadline, then restore
    // the key package when it was missing. The wrapper case models a
    // client upgrade that now supports the stored Welcome.
    db.raw_query(|conn| {
        diesel::update(incoming_envelopes::table.find((
            bo.context.installation_id().to_vec(),
            EntityKind::Welcome,
            cursor.0 as i64,
        )))
        .set((
            incoming_envelopes::blocked.eq(true),
            incoming_envelopes::error_code.eq(Some(code)),
            incoming_envelopes::unreadable_expires_at_ns.eq(Some(1i64)),
        ))
        .execute(conn)
    })
    .unwrap();

    if let Some((hash_ref, bundle)) = missing_key {
        bo.context
            .mls_storage()
            .write_key_package(&hash_ref, &bundle)
            .unwrap();
    }

    assert!(matches!(
        service
            .retry_blocked_welcomes_after(Cursor(0))
            .unwrap()
            .as_slice(),
        [WelcomeHeadOutcome::Need { .. }]
    ));

    // The row must survive: an elapsed unsupported deadline must not delete
    // a Welcome this build can now read. It rejoins ordinary processing,
    // which may still need an async dependency before it installs.
    assert!(
        db.pending_envelope(&service.topic(), cursor)
            .unwrap()
            .is_some(),
        "a supported Welcome must not be deleted by a stale deadline"
    );
    assert!(db.topic_progress(&service.topic()).unwrap().processed < cursor);
    assert!(matches!(
        service.resolve_pending_welcome(cursor).await.unwrap(),
        WelcomeHeadOutcome::Progress {
            result: Ok(Some(_)),
            ..
        }
    ));
    assert!(db.find_group(&group.group_id).unwrap().is_some());
    assert!(!db.has_pending_welcomes().unwrap());
    assert!(db.read_last_rejection(&service.topic()).unwrap().is_none());
}

/// Legacy blocked pointers retain their classification while re-fetches
/// miss, but the fixed pointer deadline still bounds missing data. At the
/// cutoff, a real fetch can recover the join; a miss must release the row.
/// Missing legacy deadlines use the pointer timestamp, never the retry time.
// verifies: JOIN-094, JOIN-063, JOIN-035
#[rstest]
#[case::recovered(false)]
#[case::still_missing(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn legacy_blocked_pointer_has_bounded_refetches(
    #[case] remains_missing: bool,
    #[values(false, true)] stored_deadline: bool,
) {
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
    let deadline = if stored_deadline {
        xmtp_common::time::now_ns() + xmtp_common::NS_IN_HOUR
    } else {
        welcome.timestamp() + WELCOME_RETENTION_NS
    };
    db.set_incoming_retry(
        &service.topic(),
        welcome.cursor,
        &IncomingRetry {
            retry_at_ns: 0,
            blocked: true,
            error_code: Some("welcome_blocked".into()),
            retry_expires_at_ns: stored_deadline.then_some(deadline),
        },
    )
    .unwrap();

    // Preserve the real pointer and keys while simulating a replica that
    // temporarily cannot return a pointee the old client had already read.
    let mut api = xmtp_api_backend::MockBackendClient::new();
    api.expect_query_raw_envelopes().returning(|_| {
        Ok(xmtp_proto::backend_v1::QueryResponse {
            envelopes: vec![],
            continuation: Some(xmtp_proto::backend_v1::Continuation { has_more: false }),
        }
        .into())
    });
    let missing_client: EngineTestPeer<_> = bo
        .with_api(std::sync::Arc::new(api), None, |options| {
            options.config_provider = Some(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::default(),
            ))
        })
        .await;
    for _ in 0..2 {
        let missing_service = WelcomeService::new(missing_client.context.clone());
        assert!(matches!(
            missing_service
                .retry_blocked_welcomes_after(Cursor(0))
                .unwrap()
                .as_slice(),
            [WelcomeHeadOutcome::Need { .. }]
        ));
        assert!(matches!(
            missing_service.resolve_pending_welcome(welcome.cursor).await.unwrap(),
            WelcomeHeadOutcome::Waiting { blocked: true, code, .. }
                if code == "welcome_blocked"
        ));
        let pending = db
            .pending_envelope(&service.topic(), welcome.cursor)
            .unwrap()
            .unwrap();
        assert!(pending.blocked);
        assert_eq!(pending.error_code.as_deref(), Some("welcome_blocked"));
        assert_eq!(pending.retry_expires_at_ns, Some(deadline));
        assert_eq!(pending.unreadable_expires_at_ns, None);
        assert!(db.read_last_rejection(&service.topic()).unwrap().is_none());
        assert!(db.find_group(&group.group_id).unwrap().is_none());
    }

    db.raw_query(|conn| {
        diesel::update(incoming_envelopes::table.find((
            bo.context.installation_id().to_vec(),
            EntityKind::Welcome,
            welcome.cursor.0 as i64,
        )))
        .set(incoming_envelopes::retry_expires_at_ns.eq(Some(1i64)))
        .execute(conn)
    })
    .unwrap();
    assert!(matches!(
        service.retry_pending_welcome(welcome.cursor, None).unwrap(),
        WelcomeHeadOutcome::Need { .. }
    ));

    if remains_missing {
        let service = WelcomeService::new(missing_client.context.clone());
        assert!(matches!(
            service
                .resolve_pending_welcome(welcome.cursor)
                .await
                .unwrap(),
            WelcomeHeadOutcome::Progress {
                result: Err(GroupError::WelcomeDataNotFound(_)),
                ..
            }
        ));
        assert!(!db.has_pending_welcomes().unwrap());
        assert!(db.find_group(&group.group_id).unwrap().is_none());
        assert_eq!(
            db.read_last_rejection(&service.topic())
                .unwrap()
                .unwrap()
                .code,
            "welcome_pointer_expired"
        );
        return;
    }

    // Even past the fetch deadline, try a legacy blocked pointer before
    // rejecting it: it may now be readable after an upgrade or recovery.
    assert!(matches!(
        service
            .resolve_pending_welcome(welcome.cursor)
            .await
            .unwrap(),
        WelcomeHeadOutcome::Progress {
            result: Ok(Some(_)),
            ..
        }
    ));
    assert!(!db.has_pending_welcomes().unwrap());
    assert!(db.find_group(&group.group_id).unwrap().is_some());
    assert!(db.read_last_rejection(&service.topic()).unwrap().is_none());
}
