use super::*;
use xmtp_db::delivery::{DeliveryCursor, HistoryPosition};
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, StoredGroupMessage};
use xmtp_mls::groups::message_list::EnrichedHistoryPage;
use xmtp_mls::messages::{decoded_message::DecodedMessage, enrichment::EnrichedStoredMessage};

fn raw_position(index: i64) -> HistoryPosition {
    HistoryPosition {
        sent_at_ns: 10,
        cursor: DeliveryCursor {
            database_id: [1; 16],
            delivery_sequence: index as u64,
        },
    }
}

pub(in crate::delivery) fn consumed(index: i64, readable: bool) -> EnrichedStoredMessage {
    let stored = StoredGroupMessage {
        id: index.to_be_bytes().repeat(4),
        group_id: vec![1; 16].try_into().unwrap(),
        decrypted_message_bytes: vec![0xff],
        sent_at_ns: 10,
        kind: GroupMessageKind::Application,
        sender_installation_id: vec![1; 32],
        sender_inbox_id: if readable {
            "0".repeat(64)
        } else {
            String::new()
        },
        delivery_status: DeliveryStatus::Published,
        content_type: ContentType::Text,
        version_major: 1,
        version_minor: 0,
        authority_id: "xmtp.org".into(),
        reference_id: None,
        sequence_id: 0,
        envelope_hash: None,
        expiry_ns: None,
        inserted_at_ns: 1,
        expire_at_ns: None,
        should_push: false,
        idempotency_key: index.to_string(),
    };
    EnrichedStoredMessage {
        decoded: DecodedMessage::from(stored.clone()),
        stored,
        delivery_cursor: Some(raw_position(index).cursor),
        parent_stored: None,
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_conversion_continuation_uses_consumed_keys() {
    for readable_count in [0, 1, 49, 50] {
        let page = lift_page(
            EnrichedHistoryPage {
                messages: (1..=50)
                    .map(|index| consumed(index, index > 50 - readable_count))
                    .collect(),
                first_position: Some(raw_position(1)),
                last_position: Some(raw_position(50)),
                has_more: true,
            },
            0,
        );
        assert_eq!(page.messages.len(), readable_count as usize);
        assert_eq!(page.skipped_count, (50 - readable_count) as u32);
        assert!(page.has_more);
        let next = page.last_position.unwrap();
        assert_eq!(
            super::super::cursor::parse(&next.delivery_cursor)?,
            raw_position(50).cursor
        );
        assert_eq!(next.sent_at.0, 10);
        let query = page_query(None, None, Some(next))?;
        assert_eq!(query.history_after, Some(raw_position(50)));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn history_page_default_compatibility_and_input_errors() {
    let page = page_query(None, None, None)?;
    assert_eq!(page.limit, Some(50));
    assert_eq!(page.direction, None);
    assert_eq!(page.delivery_status, Some(DeliveryStatus::Published));
    assert_eq!(
        page.sort_by,
        Some(xmtp_db::group_message::SortBy::SentAtDelivery)
    );
    let old: xmtp_db::group_message::MsgQueryArgs =
        crate::ListMessagesOptions::default().try_into()?;
    assert_eq!(old.limit, None);
    assert_eq!(old.direction, None);
    assert_eq!(old.delivery_status, None);
    assert_eq!(old.sort_by, None);
    assert!(old.history_before.is_none());
    assert!(old.history_after.is_none());
    for options in [
        crate::ListMessagesOptions {
            limit: Some(0),
            ..Default::default()
        },
        crate::ListMessagesOptions {
            sort_by: Some(crate::MessageSortBy::InsertedAt),
            ..Default::default()
        },
        crate::ListMessagesOptions {
            delivery_status: Some(crate::DeliveryStatus::Failed),
            ..Default::default()
        },
        crate::ListMessagesOptions {
            delivery_status: Some(crate::DeliveryStatus::Unpublished),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            page_query(Some(options), None, None),
            Err(XmtpError::InvalidArgument(_))
        ));
    }
    assert_eq!(
        page_query(
            Some(crate::ListMessagesOptions {
                limit: Some(u32::MAX),
                ..Default::default()
            }),
            None,
            None
        )?
        .limit,
        Some(i64::from(u32::MAX))
    );
    assert!(matches!(
        page_query(
            None,
            Some(MessageHistoryPosition {
                sent_at: crate::Timestamp(10),
                delivery_cursor: "bad".into()
            }),
            None
        ),
        Err(XmtpError::InvalidCursor(_))
    ));
}
