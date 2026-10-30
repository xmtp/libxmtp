//! Consent from archives written before 8.0, and the 16-byte rule.
use super::*;
use crate::tester;
use xmtp_db::consent_record::{ConsentKey, ConsentState, ConsentValue};
use xmtp_mls_common::group_metadata::DmMembers;
use xmtp_proto::xmtp::device_sync::{
    consent_backup::{ConsentSave, ConsentStateSave, ConsentTypeSave},
    group_backup::GroupSave,
    message_backup::GroupMessageSave,
};

fn dm(id: Vec<u8>, own: &str, peer: &str, last_message_ns: i64) -> BackupElement {
    let dm_id = DmMembers {
        member_one_inbox_id: own,
        member_two_inbox_id: peer,
    }
    .to_string();
    BackupElement {
        element: Some(Element::Group(GroupSave {
            id,
            created_at_ns: 1,
            membership_state: 1,
            added_by_inbox_id: own.into(),
            conversation_type: 2,
            dm_id: Some(dm_id),
            last_message_ns: Some(last_message_ns),
            ..Default::default()
        })),
    }
}

fn consent(
    entity_type: ConsentTypeSave,
    state: ConsentStateSave,
    entity: &str,
    at: i64,
) -> BackupElement {
    BackupElement {
        element: Some(Element::Consent(ConsentSave {
            entity_type: entity_type as i32,
            state: state as i32,
            entity: entity.into(),
            consented_at_ns: at,
        })),
    }
}

async fn import(
    context: &impl XmtpSharedContext,
    elements: &[BackupElement],
) -> Result<(), DeviceSyncError> {
    insert_elements(
        &mut futures::stream::iter(elements.iter().cloned().map(Ok::<_, std::io::Error>)),
        context,
    )
    .await
}

fn value(state: ConsentState, consented_at_ns: i64) -> Option<ConsentValue> {
    Some(ConsentValue {
        state,
        consented_at_ns,
    })
}

/// A pre-8.0 archive: a peer's 16-byte DM records and the peer's inbox record
/// become one inbox record with the strongest state at the greatest time, in
/// any element order. Unusable records are skipped, a failed element does not
/// stop the rest, time 0 is restored, and importing the archive again gives
/// the same records.
// verifies: CONS-051, CONS-061, ARCH-027, ARCH-016
#[xmtp_common::test(unwrap_try = true)]
async fn pre8_archive_import() {
    use ConsentStateSave::{Allowed, Denied, Unknown};
    #[allow(deprecated)]
    use ConsentTypeSave::{Address, ConversationId as Group, InboxId as Inbox};
    tester!(alix, disable_workers);
    let own = alix.inbox_id();
    let [p1, p2, p3] = ["1f", "2f", "3f"].map(|b| b.repeat(32));
    let id = |seed: u8| vec![seed; 16];
    let missing_group_message = BackupElement {
        element: Some(Element::GroupMessage(GroupMessageSave {
            id: vec![9; 32],
            group_id: vec![99; 16],
            ..Default::default()
        })),
    };
    let mut malformed = dm(id(8), own, &p3, 1);
    if let Some(Element::Group(save)) = &mut malformed.element {
        save.dm_id = Some("dm:not-a-pair".into());
    }
    #[allow(deprecated)]
    let address = Address;
    let elements = vec![
        // p1: a DM deny before its group, and a later-created inbox allow.
        consent(Group, Denied, &hex::encode_upper(id(1)), 10),
        dm(id(1), own, &p1, 1),
        consent(Inbox, Allowed, &p1.to_uppercase(), 20),
        missing_group_message,
        // p2: a DM allow after its group, and an inbox deny.
        dm(id(2), own, &p2, 1),
        consent(Group, Allowed, &hex::encode(id(2)), 30),
        consent(Inbox, Denied, &p2, 20),
        malformed,
        // p3: two DMs; the strongest state wins.
        dm(id(3), own, &p3, 5),
        dm(id(4), own, &p3, 9),
        consent(Group, Unknown, &hex::encode(id(3)), 5),
        consent(Group, Allowed, &hex::encode(id(4)), 7),
        consent(address, Denied, "0xabc", 40),
        consent(Inbox, Denied, &"e".repeat(300), 40),
        consent(Inbox, Allowed, "zero", 0),
    ];
    let expected = [
        (ConsentKey::inbox(&p1), value(ConsentState::Denied, 20)),
        (ConsentKey::inbox(&p2), value(ConsentState::Denied, 30)),
        (ConsentKey::inbox(&p3), value(ConsentState::Allowed, 7)),
        (ConsentKey::inbox("zero"), value(ConsentState::Allowed, 0)),
        (ConsentKey::inbox(&"e".repeat(300)), None),
    ];
    let db = alix.db();
    for _ in 0..2 {
        assert!(import(&alix.context, &elements).await.is_err());
        for (key, expected) in &expected {
            let held = db.consent_row(key)?.map(|row| row.value());
            assert_eq!(&held, expected, "{}", &key.entity[..4]);
        }
        for seed in 1..=4 {
            assert!(
                db.consent_row(&ConsentKey::group(&id(seed)))?.is_none(),
                "{seed}"
            );
            let dm = GroupId::try_from(id(seed).as_slice())?;
            assert!(db.find_group(&dm)?.is_some(), "a later element was skipped");
        }
        assert_eq!(
            db.consent_records()?.len(),
            4,
            "an unusable record was stored"
        );
    }
    // Imported records are owed to the other installations.
    assert!(db.consent_row(&ConsentKey::inbox(&p1))?.unwrap().owed_seq > 0);
}

/// An 8.0 archive carries no 16-byte DM group. Its 16-byte record for a DM
/// held from an earlier import merges into the peer by time, so a newer
/// inbox decision in the same archive stays.
// verifies: CONS-061
#[xmtp_common::test(unwrap_try = true)]
async fn later_archive_merges_a_held_dm_record_by_time() {
    tester!(alix, disable_workers);
    let own = alix.inbox_id();
    let peer = "5f".repeat(32);
    import(&alix.context, &[dm(vec![5; 16], own, &peer, 1)]).await?;
    let later = [
        consent(
            ConsentTypeSave::ConversationId,
            ConsentStateSave::Denied,
            &hex::encode([5; 16]),
            10,
        ),
        consent(
            ConsentTypeSave::InboxId,
            ConsentStateSave::Allowed,
            &peer,
            50,
        ),
    ];
    import(&alix.context, &later).await?;
    let held = alix
        .db()
        .consent_row(&ConsentKey::inbox(&peer))?
        .map(|row| row.value());
    assert_eq!(held, value(ConsentState::Allowed, 50));
}

/// A 32-byte DM in an archive is never pulled onto its builder's inbox: a
/// former member of group G can build a DM under G's id, and the user's
/// record for G stays under G's id after an import.
// verifies: CONS-052, CONS-061
#[xmtp_common::test(unwrap_try = true)]
async fn archive_never_moves_a_32_byte_id() {
    tester!(alix, disable_workers);
    let builder = "6f".repeat(32);
    let forged = vec![0x47; 32];
    let elements = [
        dm(forged.clone(), alix.inbox_id(), &builder, 1),
        consent(
            ConsentTypeSave::ConversationId,
            ConsentStateSave::Allowed,
            &hex::encode(&forged),
            10,
        ),
    ];
    import(&alix.context, &elements).await?;
    let db = alix.db();
    let g = ConsentKey::group(&forged);
    assert_eq!(
        db.consent_row(&g)?.map(|row| row.state),
        Some(ConsentState::Allowed)
    );
    assert!(db.consent_row(&ConsentKey::inbox(&builder))?.is_none());
    let id = GroupId::try_from(forged.as_slice())?;
    assert_eq!(alix.group(&id)?.consent_state()?, ConsentState::Unknown);
}
