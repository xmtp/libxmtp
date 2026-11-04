//! Welcome sync tests: unreadable welcomes that expire after a final attempt.

use super::*;

#[derive(Clone, Copy)]
enum UnreadableWelcome {
    Wrapper,
    MlsVersion,
    MissingKey,
}

/// Unreadable input must expire without extending its deadline, including
/// legacy blocked rows. A held Welcome does not stop admission of the next
/// one; expiry releases the Welcome barrier and key retirement.
// verifies: JOIN-087, JOIN-084, JOIN-063
#[rstest]
#[case::wrapper(UnreadableWelcome::Wrapper)]
#[case::mls_version(UnreadableWelcome::MlsVersion)]
#[case::missing_key(UnreadableWelcome::MissingKey)]
#[xmtp_common::test(unwrap_try = true)]
async fn an_unsupported_welcome_expires_after_a_final_attempt(
    #[case] failure: UnreadableWelcome,
    #[values(false, true)] previously_blocked: bool,
) {
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use openmls_traits::storage::StorageProvider;
    use xmtp_db::ConnectionExt;
    use xmtp_db::incoming_envelope::{IncomingLimits, PendingBudget};
    use xmtp_db::schema::{incoming_envelopes, key_package_history};
    use xmtp_mls_identity::key_package_maintenance::sweep_expired;
    use xmtp_proto::backend_v1::{client_envelope::Payload, welcome_message::Version};

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
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
    let cursor = welcome.cursor;
    pending_welcome_for_test(&bo.context, &welcome)
        .await
        .unwrap();
    let service = WelcomeService::new(bo.context.clone());
    let db = bo.context.db();

    let pending = db
        .pending_envelope(&service.topic(), cursor)
        .unwrap()
        .unwrap();
    let mut wire =
        xmtp_proto::backend_v1::ServerEnvelope::decode(pending.envelope.as_slice()).unwrap();
    let Payload::WelcomeMessage(message) =
        wire.envelope.as_mut().unwrap().payload.as_mut().unwrap()
    else {
        panic!("expected Welcome payload");
    };
    let Version::V1(message) = message.version.as_mut().unwrap() else {
        panic!("expected inline Welcome");
    };
    match failure {
        UnreadableWelcome::Wrapper => message.wrapper_algorithm = i32::MAX,
        UnreadableWelcome::MissingKey => message.hpke_public_key = vec![0; 32],
        UnreadableWelcome::MlsVersion => {
            (message.data, message.welcome_metadata) =
                xmtp_mls_group_keys::mls_ext::payload_encryption::wrap_payload_hpke(
                    &[0, 2],
                    &[],
                    &message.hpke_public_key,
                    xmtp_id::key_package::WrapperAlgorithm::try_from(message.wrapper_algorithm)
                        .unwrap(),
                    xmtp_configuration::WELCOME_HPKE_LABEL,
                )
                .unwrap();
        }
    }
    let row = || {
        incoming_envelopes::table.find((
            bo.context.installation_id().to_vec(),
            EntityKind::Welcome,
            cursor.0 as i64,
        ))
    };
    let envelope = wire.encode_to_vec();
    db.raw_query(|conn| {
        diesel::update(row())
            .set((
                incoming_envelopes::envelope_bytes.eq(envelope.len() as i64),
                incoming_envelopes::envelope.eq(&envelope),
                incoming_envelopes::blocked.eq(previously_blocked),
                incoming_envelopes::error_code.eq(previously_blocked.then_some("welcome_blocked")),
            ))
            .execute(conn)
    })
    .unwrap();

    // First pass blocks the row and records a retention deadline.
    let before = xmtp_common::time::now_ns();
    if previously_blocked {
        service.retry_blocked_welcomes_after(Cursor(0)).unwrap();
    } else {
        service.process_pending_welcomes_once(&[]).unwrap();
    }
    let blocked = db
        .pending_envelope(&service.topic(), cursor)
        .unwrap()
        .unwrap();
    assert!(blocked.blocked);
    let deadline = blocked.unreadable_expires_at_ns.unwrap();
    assert!(deadline >= before + xmtp_common::NS_IN_DAY);
    assert!(deadline <= xmtp_common::time::now_ns() + xmtp_common::NS_IN_DAY);
    assert!(db.has_pending_welcomes().unwrap());

    // Before the deadline the row is retained, not removed.
    service.retry_blocked_welcomes_after(Cursor(0)).unwrap();
    let retained = db.pending_envelope(&service.topic(), cursor).unwrap();
    assert_eq!(
        retained.expect("retained").unreadable_expires_at_ns,
        Some(deadline),
        "a retry must not extend the first deadline"
    );

    let history = db
        .find_key_package_history_entries_before_id(i32::MAX)
        .unwrap()
        .pop()
        .unwrap();
    let hash =
        xmtp_mls_core::identity::deserialize_key_package_hash_ref(&history.key_package_hash_ref)
            .unwrap();
    db.raw_query(|conn| {
        diesel::update(key_package_history::table.find(history.id))
            .set(key_package_history::delete_at_ns.eq(0))
            .execute(conn)
    })
    .unwrap();
    sweep_expired(&bo.context).unwrap();
    let retained: Option<openmls::prelude::KeyPackageBundle> =
        bo.context.mls_storage().key_package(&hash).unwrap();
    assert!(retained.is_some());

    // A held Welcome uses the held allowance, so one row of ready
    // capacity still admits the next Welcome.
    let next_group = alix.create_group(None, None).unwrap();
    next_group.invite(&bo).await.unwrap();
    let budget = PendingBudget {
        rows: 1,
        bytes: u64::MAX,
    };
    let limits = IncomingLimits {
        batch: budget,
        topic: budget,
        kind: budget,
    };
    let store = MlsStore::new(bo.context.clone());
    let topics = [Topic::new_welcome_message(bo.context.installation_id())];
    store.receive_topics_once(&topics, limits).await.unwrap();
    assert!(db.topic_progress(&service.topic()).unwrap().received > cursor);

    // At the deadline the final attempt runs and the row is removed, so the
    // Welcome barrier and key retirement are released.
    db.raw_query(|conn| {
        diesel::update(row())
            .set(incoming_envelopes::unreadable_expires_at_ns.eq(Some(1i64)))
            .execute(conn)
    })
    .unwrap();
    let outcomes = service.retry_blocked_welcomes_after(Cursor(0)).unwrap();
    let WelcomeHeadOutcome::Progress {
        result: Err(error), ..
    } = outcomes.into_iter().next().unwrap()
    else {
        panic!("expected terminal rejection");
    };
    match failure {
        UnreadableWelcome::Wrapper => {
            assert!(matches!(error, GroupError::UnsupportedWelcomeVersion(_)))
        }
        UnreadableWelcome::MlsVersion => assert!(matches!(
            error,
            GroupError::WelcomeError(openmls::prelude::WelcomeError::UnsupportedMlsVersion)
        )),
        UnreadableWelcome::MissingKey => assert!(matches!(
            error,
            GroupError::NotFound(xmtp_db::NotFound::KeyPackageReference(_))
        )),
    }
    assert_eq!(
        db.read_last_rejection(&service.topic())
            .unwrap()
            .unwrap()
            .code,
        "unsupported_welcome_expired"
    );
    assert!(
        db.pending_envelope(&service.topic(), cursor)
            .unwrap()
            .is_none()
    );
    assert!(db.topic_progress(&service.topic()).unwrap().processed >= cursor);
    assert!(
        db.welcome_barrier_complete(&service.topic(), cursor)
            .unwrap()
    );

    let next = db
        .first_pending_envelope(&service.topic())
        .unwrap()
        .unwrap();
    assert!(matches!(
        service
            .resolve_pending_welcome(Cursor(next.sequence_id as u64))
            .await
            .unwrap(),
        WelcomeHeadOutcome::Progress {
            result: Ok(Some(_)),
            ..
        }
    ));
    assert!(db.find_group(&next_group.group_id).unwrap().is_some());
    sweep_expired(&bo.context).unwrap();
    let removed: Option<openmls::prelude::KeyPackageBundle> =
        bo.context.mls_storage().key_package(&hash).unwrap();
    assert!(removed.is_none());
}
