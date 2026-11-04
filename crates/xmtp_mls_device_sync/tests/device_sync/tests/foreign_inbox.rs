//! Device sync tests: foreign inbox.

use super::*;

// verifies: SYNC-010
#[rstest::rstest]
#[case::another_inbox_adds(false)]
#[case::another_inbox_is_a_leaf(true)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_welcome_naming_another_inbox_is_rejected(#[case] own_adder: bool) {
    use xmtp_mls_engine::groups::test_support::crafted_welcome::{
        CraftedWelcome, send_crafted_welcome_with,
    };
    use xmtp_mls_group_keys::group_keys::{GroupKeys, GroupRoot};
    use xmtp_proto::types::Cursor;

    tester!(alix1, disable_workers);
    tester!(eve, disable_workers);
    let alix2 = if own_adder {
        tester!(alix2, from: alix1, disable_workers);
        Some(alix2)
    } else {
        None
    };
    let root = GroupRoot::generate();
    let group_id = GroupKeys::derive(&root).group_id();
    // A current client rejects its own commit that adds eve to a sync group,
    // so the Welcome is crafted. The adder is always a leaf, so no Welcome
    // names only a foreign adder.
    let (adder, other_member) = match &alix2 {
        Some(alix2) => (alix2, Some(&eve)),
        None => (&eve, None),
    };
    let options = CraftedWelcome {
        conversation_type: ConversationType::Sync,
        other_member,
        ..Default::default()
    };
    send_crafted_welcome_with(adder, &alix1, &group_id, Some(&root), options).await;

    alix1.wait_for_newest_welcome_rejected(Cursor(0)).await;

    let foreign_id = GroupId::from(group_id);
    assert_welcome_rejected(&alix1, &foreign_id);
    let target = alix1.device_sync_client().get_sync_group().await.unwrap();
    assert_ne!(target.group_id, foreign_id);
}

// verifies: SYNC-010
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_welcome_from_own_installation_is_joined() {
    tester!(alix1);
    tester!(alix2, from: alix1);
    let alix2_group = alix2.device_sync_client().get_sync_group().await?;

    alix1.sync_welcomes().await?;

    let stored = alix1.context.db().find_group(&alix2_group.group_id)??;
    assert_eq!(stored.membership_state, GroupMembershipState::Allowed);
    assert_eq!(stored.added_by_inbox_id, alix1.inbox_id());
    let target = alix1.device_sync_client().get_sync_group().await?;
    assert_eq!(target.group_id, alix2_group.group_id);
}

// verifies: SYNC-010
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_group_commit_adding_another_inbox_is_rejected() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(eve);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    let alix2_group = alix2.group(&group.group_id)?;
    let epoch = alix2_group.epoch().await?;

    // alix1's own check rejects the commit after it is published.
    assert!(group.add_members(&[eve.inbox_id()]).await.is_err());
    let topic = xmtp_db::incoming_envelope::StreamTopic::group(group.group_id);
    let db = alix2.context.db();
    let rejection = xmtp_common::wait_for_some(|| async {
        let _ = alix2_group.sync().await;
        db.read_last_rejection(&topic).unwrap()
    })
    .await
    .expect("alix2 did not reject the commit");
    assert_eq!(rejection.code, "commit_validation_failure");

    let members = alix2_group.members().await?;
    assert!(members.iter().all(|m| m.inbox_id != eve.inbox_id()));
    assert_eq!(alix2_group.epoch().await?, epoch);
}

/// The part of a stored sync group that names another inbox.
#[derive(Clone, Copy)]
enum Foreign {
    Adder,
    Leaf,
    AdderAndLeaf,
}

// verifies: SYNC-010
#[rstest::rstest]
#[case::adder(Foreign::Adder)]
#[case::leaf(Foreign::Leaf)]
#[case::adder_and_leaf(Foreign::AdderAndLeaf)]
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn stored_sync_group_naming_another_inbox_is_never_the_target(#[case] foreign: Foreign) {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix);
    tester!(eve);
    let own_id = alix
        .device_sync_client()
        .get_sync_group()
        .await
        .unwrap()
        .group_id;
    // Stands in for a foreign sync group that an older build stored.
    let foreign_id = match foreign {
        Foreign::Adder => alix.create_group(None, None).unwrap().group_id,
        Foreign::Leaf => {
            alix.create_group_with_members(&[eve.inbox_id()], None, None)
                .await
                .unwrap()
                .group_id
        }
        Foreign::AdderAndLeaf => {
            let group = eve
                .create_group_with_members(&[alix.inbox_id()], None, None)
                .await
                .unwrap();
            alix.sync_welcomes().await.unwrap();
            group.group_id
        }
    };
    let added_by = match foreign {
        Foreign::Leaf => alix.inbox_id(),
        Foreign::Adder | Foreign::AdderAndLeaf => eve.inbox_id(),
    };
    alix.context
        .db()
        .raw_query(|conn| {
            diesel::update(dsl::groups.find(&foreign_id))
                .set((
                    dsl::conversation_type.eq(ConversationType::Sync),
                    dsl::created_at_ns.eq(i64::MAX),
                    dsl::added_by_inbox_id.eq(added_by),
                ))
                .execute(conn)
        })
        .unwrap();

    let target = alix.device_sync_client().get_sync_group().await.unwrap();
    assert_eq!(target.group_id, own_id);
}

// verifies: SYNC-030, SYNC-010
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sync_message_from_another_inbox_is_not_applied() {
    use diesel::prelude::*;
    use xmtp_db::schema::groups::dsl;

    tester!(alix1);
    tester!(alix2, from: alix1);
    tester!(eve);
    let eve_group = eve
        .create_group_with_members(&[alix1.inbox_id()], None, None)
        .await?;
    alix1.sync_welcomes().await?;
    alix2.sync_welcomes().await?;
    // Stands in for a foreign sync group that an older build accepted.
    alix1.context.db().raw_query(|conn| {
        diesel::update(dsl::groups.find(&eve_group.group_id))
            .set(dsl::conversation_type.eq(ConversationType::Sync))
            .execute(conn)
    })?;

    // Eve's key has the later cycle time, so it wins if it is applied.
    let update = |entity: &str, key: u8, cycled_at_ns: i64| {
        let mut bad = consent_update(ConsentType::InboxId, ConsentState::Denied, entity);
        if let Some(PreferenceUpdateProto::Consent(record)) = &mut bad.update {
            record.entity_type = 3; // Retired ADDRESS record.
        }
        sync_bytes(vec![
            bad,
            consent_update(ConsentType::InboxId, ConsentState::Denied, entity),
            root_key_update(vec![key; 42], cycled_at_ns),
        ])
    };
    let now = now_ns();
    eve_group
        .send_message(
            &update("from-eve", 1, now + NS_IN_HOUR),
            SendMessageOpts::default(),
        )
        .await?;
    alix2
        .group(&eve_group.group_id)?
        .send_message(&update("from-alix2", 2, now), SendMessageOpts::default())
        .await?;
    alix1.group(&eve_group.group_id)?.sync().await?;

    let db = alix1.context.db();
    let pending = db.unprocessed_sync_group_messages(None)?;
    assert!(
        pending
            .iter()
            .any(|message| message.sender_inbox_id == eve.inbox_id())
    );
    alix1
        .device_sync_client()
        .process_sync_group_messages()
        .await?;

    assert!(held(&alix1, &ConsentKey::inbox("from-eve")).is_none());
    assert_eq!(
        held(&alix1, &ConsentKey::inbox("from-alix2"))
            .unwrap()
            .state,
        ConsentState::Denied
    );
    assert_eq!(
        StoredUserPreferences::load(&db)?.hmac_key,
        Some(vec![2; 42].into())
    );
    assert!(
        db.unprocessed_sync_group_messages(None)?
            .iter()
            .all(|message| message.sender_inbox_id != eve.inbox_id())
    );
}

// verifies: SYNC-016
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn failing_sync_message_stays_pending() {
    use diesel::RunQueryDsl;
    use xmtp_db::processed_device_sync_messages::SYNC_MESSAGE_PAGE;
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    join_sync_group(&alix2, &[&alix1]).await?;
    let db = alix1.context.db();
    db.raw_query(|conn| {
        diesel::sql_query(
            "CREATE TRIGGER fail_sync_consent BEFORE INSERT ON consent_records \
         WHEN NEW.entity = 'retry' BEGIN SELECT RAISE(ABORT, 'injected sync failure'); END",
        )
        .execute(conn)
    })?;
    let retry = ConsentKey::inbox("retry");
    decide_at(&alix2, &retry, ConsentState::Denied, now_ns())?;
    publish(&alix2).await?;
    // Later rows still apply in the same pass while `retry` stays Pending.
    let more: Vec<_> = (0..SYNC_MESSAGE_PAGE)
        .map(|i| ConsentKey::inbox(&format!("more-{i}")))
        .collect();
    for key in &more {
        decide_at(&alix2, key, ConsentState::Allowed, now_ns())?;
        publish(&alix2).await?;
    }
    receive(&alix1).await?;
    for key in &more {
        assert!(held(&alix1, key).is_some());
    }
    for _ in 0..4 {
        receive(&alix1).await?;
    }
    assert_eq!(db.unprocessed_sync_group_messages(None)?.len(), 1);
    assert!(held(&alix1, &retry).is_none());
    db.raw_query(|conn| diesel::sql_query("DROP TRIGGER fail_sync_consent").execute(conn))?;
    receive(&alix1).await?;
    assert_eq!(held(&alix1, &retry), held(&alix2, &retry));
    assert!(db.unprocessed_sync_group_messages(None)?.is_empty());
}

/// A message an own installation sent before its revocation is applied, also
/// when the receiver processes it after the revocation. One sent after it is
/// dropped.
// verifies: SYNC-030
#[xmtp_common::test(unwrap_try = true)]
#[cfg_attr(target_arch = "wasm32", ignore)]
async fn sender_judged_at_envelope_time() {
    tester!(alix1, disable_workers);
    tester!(alix2, from: alix1, disable_workers);
    tester!(alix3, from: alix1, disable_workers);
    let group = alix1.device_sync_client().get_sync_group().await?;
    alix2.sync_welcomes().await?;
    alix3.sync_welcomes().await?;
    let before = ConsentKey::inbox("sent-before-revocation");
    decide_at(&alix2, &before, ConsentState::Denied, now_ns())?;
    publish(&alix2).await?;
    revoke(&alix3, &alix2).await?;
    let after = sync_bytes(vec![consent_update(
        ConsentType::InboxId,
        ConsentState::Denied,
        "sent-after-revocation",
    )]);
    alix2
        .group(&group.group_id)?
        .send_message(&after, SendMessageOpts::default())
        .await?;

    receive(&alix1).await?;
    assert_eq!(held(&alix1, &before), held(&alix2, &before));
    assert!(held(&alix1, &ConsentKey::inbox("sent-after-revocation")).is_none());
    assert!(
        alix1
            .context
            .db()
            .unprocessed_sync_group_messages(None)?
            .is_empty()
    );
}
