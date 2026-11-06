//! Welcome sync tests: the group-prefix wait and which failures retain a welcome.

use super::*;

/// A group-prefix hold gets a deadline 3 days after the Welcome's backend
/// timestamp and keeps key material until the hold ends. Classifying it
/// past the deadline still holds it. Expiry needs the hold's row, a passed
/// deadline, and a processed position at the target and below the anchor;
/// it releases the Welcome barrier and the key package and leaves the
/// held group unchanged.
// verifies: JOIN-092
#[xmtp_common::test(unwrap_try = true)]
async fn group_prefix_wait_expires_after_three_days() {
    use diesel::{ExpressionMethods, QueryDsl, RunQueryDsl};
    use openmls_traits::storage::StorageProvider;
    use xmtp_db::ConnectionExt;
    use xmtp_db::schema::{incoming_envelopes, key_package_history};
    use xmtp_mls_identity::key_package_maintenance::sweep_expired;

    let alix = EngineTestPeer::new().await;
    let bo = EngineTestPeer::new().await;
    let HeldReadd {
        bo_group, welcome, ..
    } = held_readd(&alix, &bo).await;
    let service = WelcomeService::new(bo.context.clone());
    let db = bo.context.db();
    let held = || -> Result<bool, GroupError> {
        Ok(db
            .pending_envelope(&service.topic(), welcome.cursor)?
            .is_some())
    };

    let WelcomeHeadOutcome::Need {
        requirement:
            WelcomeRequirement::GroupPrefix {
                group_id,
                anchor,
                deadline_ns,
            },
        ..
    } = service.retry_pending_welcome(welcome.cursor, None)?
    else {
        panic!("expected a group-prefix hold");
    };
    assert_eq!(group_id, bo_group.group_id);
    assert_eq!(deadline_ns, welcome.timestamp() + WELCOME_RETENTION_NS);
    let row = db
        .pending_envelope(&service.topic(), welcome.cursor)?
        .unwrap();
    assert_eq!(row.error_code.as_deref(), Some("group_prefix"));
    assert!(!row.blocked);
    assert_eq!(row.retry_expires_at_ns, Some(deadline_ns));
    assert!(!db.welcome_barrier_complete(&service.topic(), welcome.cursor)?);
    let processed = db.topic_progress(&StreamTopic::group(group_id))?.processed;
    assert!(processed < anchor);
    let expire = |anchor, target| {
        service.expire_group_prefix_welcome(welcome.cursor, group_id, anchor, target)
    };

    let history = db
        .find_key_package_history_entries_before_id(i32::MAX)?
        .pop()
        .unwrap();
    let hash =
        xmtp_mls_core::identity::deserialize_key_package_hash_ref(&history.key_package_hash_ref)?;
    db.raw_query(|conn| {
        diesel::update(key_package_history::table.filter(key_package_history::id.eq(history.id)))
            .set(key_package_history::delete_at_ns.eq(0))
            .execute(conn)
    })?;
    sweep_expired(&bo.context)?;
    let retained: Option<openmls::prelude::KeyPackageBundle> =
        bo.context.mls_storage().key_package(&hash)?;
    assert!(
        retained.is_some(),
        "a prefix hold keeps expired private keys"
    );

    // Before the deadline the hold cannot expire.
    assert!(matches!(
        expire(anchor, processed)?,
        WelcomeHeadOutcome::Waiting { .. }
    ));
    assert!(held()?);

    // Past the deadline, a row that is no longer a group-prefix hold
    // cannot expire.
    db.raw_query(|conn| {
        diesel::update(incoming_envelopes::table.find((
            bo.context.installation_id().to_vec(),
            EntityKind::Welcome,
            welcome.cursor.0 as i64,
        )))
        .set((
            incoming_envelopes::retry_expires_at_ns.eq(Some(1i64)),
            incoming_envelopes::error_code.eq("identity_dependency"),
        ))
        .execute(conn)
    })?;
    assert!(matches!(
        expire(anchor, processed)?,
        WelcomeHeadOutcome::Waiting { .. }
    ));
    assert!(held()?);

    // Classification keeps the stored deadline and still holds the
    // Welcome until the coordinator decides.
    assert!(matches!(
        service.retry_pending_welcome(welcome.cursor, None)?,
        WelcomeHeadOutcome::Need {
            requirement: WelcomeRequirement::GroupPrefix { deadline_ns: 1, .. },
            ..
        }
    ));
    assert_eq!(
        db.pending_envelope(&service.topic(), welcome.cursor)?
            .unwrap()
            .retry_expires_at_ns,
        Some(1)
    );

    // Processing has not reached the target, or is not below the anchor.
    for (anchor, target) in [(anchor, Cursor(processed.0 + 1)), (processed, processed)] {
        assert!(matches!(
            expire(anchor, target)?,
            WelcomeHeadOutcome::Waiting { .. }
        ));
        assert!(held()?);
    }

    let epoch = bo_group.epoch().await?;
    assert!(matches!(
        expire(anchor, processed)?,
        WelcomeHeadOutcome::Progress {
            result: Err(GroupError::WelcomeGroupPrefixPending { .. }),
            ..
        }
    ));
    assert!(db.welcome_barrier_complete(&service.topic(), welcome.cursor)?);
    assert_eq!(
        db.read_last_rejection(&service.topic())?.unwrap().code,
        "group_prefix_expired"
    );
    assert!(!held()?);
    assert!(bo_group.is_active()?);
    assert_eq!(bo_group.epoch().await?, epoch);
    sweep_expired(&bo.context)?;
    let retired: Option<openmls::prelude::KeyPackageBundle> =
        bo.context.mls_storage().key_package(&hash)?;
    assert!(retired.is_none(), "key is retired once the hold ends");
}

enum WelcomeRetention {
    Unreadable,
    Held,
    Terminal,
}

/// Only unreadable MLS formats and missing Welcome key material get this
/// deadline; local failures and required group versions keep their policy.
// verifies: JOIN-087, JOIN-084, JOIN-085, JOIN-048, JOIN-027, JOIN-043
#[rstest]
#[case::mls_version(|| openmls::prelude::WelcomeError::UnsupportedMlsVersion.into(), WelcomeRetention::Unreadable)]
#[case::extensions(|| openmls::prelude::WelcomeError::UnsupportedExtensions.into(), WelcomeRetention::Unreadable)]
#[case::capability(|| openmls::prelude::WelcomeError::UnsupportedCapability.into(), WelcomeRetention::Unreadable)]
#[case::ciphersuite(|| openmls::prelude::WelcomeError::UnsupportedCiphersuite(openmls::prelude::Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519).into(), WelcomeRetention::Unreadable)]
#[case::key_reference(|| xmtp_db::NotFound::KeyPackageReference(vec![]).into(), WelcomeRetention::Unreadable)]
#[case::key_package(|| xmtp_db::NotFound::KeyPackage(vec![]).into(), WelcomeRetention::Unreadable)]
#[case::post_quantum_key(|| xmtp_db::NotFound::PostQuantumPrivateKey.into(), WelcomeRetention::Unreadable)]
#[case::group_version(|| GroupError::UnsupportedWelcomeVersion("999.0.0".into()), WelcomeRetention::Held)]
#[case::local_failure(|| GroupError::LockUnavailable, WelcomeRetention::Held)]
#[case::unrelated_missing_state(|| xmtp_db::NotFound::MlsGroup(GroupId::from([1; 16])).into(), WelcomeRetention::Held)]
#[case::psk(|| openmls::prelude::WelcomeError::Psk(openmls::schedule::errors::PskError::KeyNotFound).into(), WelcomeRetention::Terminal)]
#[case::stale_join_anchor(|| xmtp_db::StorageError::Stream(xmtp_db::stream_storage::StreamStorageError::StaleJoinAnchor).into(), WelcomeRetention::Terminal)]
#[xmtp_common::test(unwrap_try = true)]
async fn welcome_retention_is_limited_to_unreadable_input(
    #[case] error: fn() -> GroupError,
    #[case] expectation: WelcomeRetention,
    #[values(false, true)] prior_pointer_hold: bool,
) {
    use xmtp_db::ConnectionExt;
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
    )
    .unwrap();
    if prior_pointer_hold {
        db.set_incoming_retry(
            &service.topic(),
            Cursor(1),
            &IncomingRetry {
                retry_at_ns: 0,
                blocked: false,
                error_code: Some("welcome_pointee".into()),
                retry_expires_at_ns: Some(1),
            },
        )
        .unwrap();
    }
    let pending = db
        .pending_envelope(&service.topic(), Cursor(1))
        .unwrap()
        .unwrap();
    let outcome = service
        .classify_pending_result(&pending, Err(error()))
        .unwrap();
    if matches!(expectation, WelcomeRetention::Terminal) {
        assert!(matches!(
            outcome,
            WelcomeHeadOutcome::Progress { result: Err(_), .. }
        ));
        let rejection = db.read_last_rejection(&service.topic()).unwrap();
        assert_eq!(rejection.unwrap().code, "invalid_welcome");
        assert!(
            db.pending_envelope(&service.topic(), Cursor(1))
                .unwrap()
                .is_none()
        );
        assert!(!db.has_pending_welcomes().unwrap());
        return;
    }
    assert!(matches!(outcome, WelcomeHeadOutcome::Waiting { .. }));
    let pending = db
        .pending_envelope(&service.topic(), Cursor(1))
        .unwrap()
        .unwrap();
    let bounded = matches!(expectation, WelcomeRetention::Unreadable);
    assert_eq!(pending.unreadable_expires_at_ns.is_some(), bounded);
    if bounded {
        assert!(pending.blocked);
        assert!(pending.unreadable_expires_at_ns.unwrap() > xmtp_common::time::now_ns());
        // Present the final attempt with a persisted elapsed deadline.
        db.raw_query(|conn| {
            use diesel::{ExpressionMethods, RunQueryDsl};
            use xmtp_db::schema::incoming_envelopes;
            diesel::update(incoming_envelopes::table)
                .set(incoming_envelopes::unreadable_expires_at_ns.eq(Some(1i64)))
                .execute(conn)
        })
        .unwrap();
        let pending = db
            .pending_envelope(&service.topic(), Cursor(1))
            .unwrap()
            .unwrap();
        assert!(matches!(
            service
                .classify_pending_result(&pending, Err(error()))
                .unwrap(),
            WelcomeHeadOutcome::Progress { result: Err(_), .. }
        ));
        assert!(!db.has_pending_welcomes().unwrap());
    }
}
