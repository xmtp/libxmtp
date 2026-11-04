//! The leaves of `groups/tests/test_welcome_pointers.rs` that need the client.

use crate::context::CoreContext;
use crate::groups::test_support::welcome_pointers::{
    PendingWelcomeOutcome, publish_commit_without_welcome, publish_pointer, random_pointer,
    resolve_pending_welcome, retry_pending_welcome, sealed_pointee,
};
use crate::tester;
use futures::StreamExt;
use std::time::Duration;
use xmtp_db::group::QueryGroup;
use xmtp_db::incoming_envelope::QueryIncomingEnvelope;

/// Missing pointees retain their fixed fetch deadline and can later join. If a
/// fetched pointee has an unsupported MLS version, it receives a separate full
/// day of retention whose deadline survives retries past the fetch deadline.
/// Transiently missing pointees and legacy retry states cannot shorten that hold;
/// if the pointee remains missing after the hold expires, it is rejected.
// verifies: JOIN-094, JOIN-087, JOIN-063
#[xmtp_common::timeout(Duration::from_secs(40))]
#[rstest::rstest]
#[case::supported(false)]
#[case::unsupported_mls_version(true)]
#[xmtp_common::test(unwrap_try = true)]
async fn test_welcome_pointer_pending_retry_resolution(#[case] unsupported: bool) {
    tester!(alix, disable_workers);
    tester!(bo, disable_workers);

    let group = crate::groups::MlsGroup::create_and_insert(
        alix.context.clone(),
        xmtp_db::group::ConversationType::Group,
        crate::groups::group_permissions::PolicySet::default(),
        xmtp_mls_group_state::group::GroupMetadataOptions::default(),
        None,
    )
    .unwrap();

    // Have to sync the group otherwise bo won't find it and it won't get created
    group.sync().await.unwrap();

    tracing::info!("Creating welcome for group");
    // Publish the commit but not its Welcome, so the test controls delivery.
    let (welcome_message, join_anchor) =
        publish_commit_without_welcome(&group, bo.inbox_id()).await;

    let welcome_pointer_v1 = random_pointer();
    let welcome_from_api = publish_pointer(&alix, &bo, &welcome_pointer_v1).await;
    let service = crate::groups::welcome_sync::WelcomeService::new(bo.context.clone());
    assert!(matches!(
        resolve_pending_welcome(&service, welcome_from_api.cursor)
            .await
            .unwrap(),
        PendingWelcomeOutcome::Waiting { blocked: false, .. }
    ));

    let topic = bo.welcome_topic();
    let pending = bo
        .context
        .db()
        .pending_envelope(&topic, welcome_from_api.cursor)
        .unwrap()
        .unwrap();
    assert_eq!(pending.error_code.as_deref(), Some("welcome_pointee"));
    let first_expiry = pending.retry_expires_at_ns.unwrap();
    assert_eq!(
        first_expiry,
        welcome_from_api.timestamp() + 3 * xmtp_common::NS_IN_DAY
    );

    let welcome_data = sealed_pointee(
        &welcome_pointer_v1,
        if unsupported {
            &[0, 2]
        } else {
            &welcome_message
        },
        join_anchor,
    );

    // A second attempt must keep the first pointer deadline.
    let _ = resolve_pending_welcome(&service, welcome_from_api.cursor)
        .await
        .unwrap();
    assert_eq!(
        bo.context
            .db()
            .pending_envelope(&topic, welcome_from_api.cursor)
            .unwrap()
            .unwrap()
            .retry_expires_at_ns,
        Some(first_expiry)
    );

    tracing::info!("Sending welcome to where welcome pointer resolves to");
    alix.context
        .api()
        .send_welcome_messages(&[welcome_data])
        .await
        .unwrap();
    if unsupported {
        use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
        use xmtp_db::{ConnectionExt, schema::incoming_envelopes};

        let db = bo.context.db();
        let row = || {
            incoming_envelopes::table.find((
                bo.context.installation_id().to_vec(),
                xmtp_db::refresh_state::EntityKind::Welcome,
                welcome_from_api.cursor.0 as i64,
            ))
        };
        // Simulate resolving the pointee in the final hour of its fetch window.
        let before = xmtp_common::time::now_ns();
        let pointer_deadline = before + xmtp_common::NS_IN_HOUR;
        db.raw_query(|conn| {
            diesel::update(row())
                .set(incoming_envelopes::retry_expires_at_ns.eq(Some(pointer_deadline)))
                .execute(conn)
        })
        .unwrap();
        assert!(matches!(
            resolve_pending_welcome(&service, welcome_from_api.cursor)
                .await
                .unwrap(),
            PendingWelcomeOutcome::Waiting { blocked: true, .. }
        ));
        let pending = db
            .pending_envelope(&topic, welcome_from_api.cursor)
            .unwrap()
            .unwrap();
        let deadline = pending.unreadable_expires_at_ns.unwrap();
        assert!(deadline >= before + xmtp_common::NS_IN_DAY);
        assert!(deadline <= xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY);
        assert_eq!(pending.retry_expires_at_ns, Some(pointer_deadline));

        // Keep the real Welcome and keys, but make pointee queries temporarily
        // return no data, as a lagging replica can after a previously good read.
        let mut api = xmtp_api_backend::MockBackendClient::new();
        api.expect_query_raw_envelopes().returning(|_| {
            Ok(xmtp_proto::backend_v1::QueryResponse {
                envelopes: vec![],
                continuation: Some(xmtp_proto::backend_v1::Continuation { has_more: false }),
            }
            .into())
        });
        let missing_client = crate::builder::ClientBuilder::from_client(bo.client.clone())
            .api_client(std::sync::Arc::new(api))
            .config_provider(std::sync::Arc::new(
                xmtp_configuration::StaticConfigProvider::default(),
            ))
            .with_allow_offline(Some(true))
            .with_disable_workers(true)
            .build()
            .await
            .unwrap();
        let missing_service =
            crate::groups::welcome_sync::WelcomeService::new(missing_client.context.clone());
        assert!(matches!(
            resolve_pending_welcome(&missing_service, welcome_from_api.cursor)
                .await
                .unwrap(),
            PendingWelcomeOutcome::Waiting { blocked: true, .. }
        ));

        // Passing the fetch deadline cannot shorten the unreadable hold, and
        // a previously persisted pointee retry must not skip its next attempt.
        db.raw_query(|conn| {
            diesel::update(row())
                .set((
                    incoming_envelopes::retry_expires_at_ns.eq(Some(1i64)),
                    incoming_envelopes::error_code.eq(Some("welcome_pointee")),
                ))
                .execute(conn)
        })
        .unwrap();
        assert!(matches!(
            retry_pending_welcome(&service, welcome_from_api.cursor).unwrap(),
            PendingWelcomeOutcome::Need { .. }
        ));
        assert!(matches!(
            resolve_pending_welcome(&missing_service, welcome_from_api.cursor)
                .await
                .unwrap(),
            PendingWelcomeOutcome::Waiting { blocked: true, .. }
        ));
        // The real backend recovers. Rechecking the MLS payload still keeps the
        // original unreadable deadline, including across service recreation.
        let service = crate::groups::welcome_sync::WelcomeService::new(bo.context.clone());
        assert!(matches!(
            resolve_pending_welcome(&service, welcome_from_api.cursor)
                .await
                .unwrap(),
            PendingWelcomeOutcome::Waiting { blocked: true, .. }
        ));
        assert_eq!(
            db.pending_envelope(&topic, welcome_from_api.cursor)
                .unwrap()
                .unwrap()
                .unreadable_expires_at_ns,
            Some(deadline)
        );
        db.raw_query(|conn| {
            diesel::update(row())
                .set(incoming_envelopes::unreadable_expires_at_ns.eq(Some(1i64)))
                .execute(conn)
        })
        .unwrap();
        assert!(matches!(
            resolve_pending_welcome(&missing_service, welcome_from_api.cursor)
                .await
                .unwrap(),
            PendingWelcomeOutcome::Progress {
                result: Err(crate::groups::GroupError::WelcomeDataNotFound(_)),
                ..
            }
        ));
        assert!(!db.has_pending_welcomes().unwrap());
        assert!(db.find_group(&group.group_id).unwrap().is_none());
        assert_eq!(
            db.read_last_rejection(&topic).unwrap().unwrap().code,
            "unsupported_welcome_expired"
        );
        return;
    }

    let events = bo.context.events().subscribe(
        xmtp_events::EventFilter::new([xmtp_events::EventKind::ConversationJoined]),
        Some(10),
    );
    let conversations = bo.stream_conversations(None, true).await.unwrap();
    tokio::pin!(conversations);

    bo.sync_welcomes().await.unwrap();

    // TODO subscribe to all messages and then assert that group is received.

    tracing::info!("Receiving event for new group");
    let event = xmtp_common::time::timeout(std::time::Duration::from_secs(10), events.next())
        .await
        .unwrap()
        .unwrap();
    match event.client {
        Some(xmtp_events::ClientEvent::ConversationJoined(joined)) => {
            assert_eq!(joined.group_id, group.group_id.to_vec());
        }
        e => panic!("Expected NewGroup event, got {:?}", e),
    }

    tracing::info!("Finding group for bo");
    let bo_group = bo
        .find_groups(xmtp_db::group::GroupQueryArgs::default())
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(bo_group.group_id, group.group_id);

    let stored_group = bo
        .context
        .db()
        .find_group_by_sequence_id(welcome_from_api.cursor)
        .unwrap()
        .unwrap();
    assert_eq!(stored_group.id.as_slice(), bo_group.group_id.as_slice());

    tracing::info!("Verifying message is received in conversation stream");
    let conversation_group =
        xmtp_common::time::timeout(std::time::Duration::from_secs(10), conversations.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    assert_eq!(conversation_group.group_id, bo_group.group_id);
}
