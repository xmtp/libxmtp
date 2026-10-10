use super::*;
use crate::{DeliveryStatus, ListMessagesOptions, MessageKind, MessageSortBy};
use xmtp_mls::groups::message_list::EnrichedRecoveryPage;

#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_conversion_keeps_raw_positions_and_sentinel() {
    for readable_count in [0, 1, 49, 50] {
        let first = RecoveryPosition {
            sent_at_ns: -10,
            database_id: [1; 16],
            message_id: vec![],
        };
        let last = RecoveryPosition {
            sent_at_ns: -10,
            database_id: [1; 16],
            message_id: vec![255],
        };
        let page = lift_recovery_page(
            EnrichedRecoveryPage {
                messages: (1..=50)
                    .map(|index| {
                        super::super::history::tests::consumed(index, index > 50 - readable_count)
                    })
                    .collect(),
                first_position: Some(first.clone()),
                last_position: Some(last.clone()),
                has_more: true,
            },
            0,
        );
        assert_eq!(page.messages.len(), readable_count as usize);
        assert_eq!(page.skipped_count, (50 - readable_count) as u32);
        assert!(page.has_more);
        let query = recovery_query(None, page.first_position, page.last_position)?;
        assert_eq!(query.before, Some(first));
        assert_eq!(query.after, Some(last));
    }
}

#[xmtp_common::test(unwrap_try = true)]
async fn recovery_page_options_and_token_errors() {
    let defaults = recovery_query(None, None, None)?;
    assert_eq!(defaults.messages.limit, Some(50));
    assert!(defaults.messages.kind.is_none() && defaults.messages.delivery_status.is_none());
    assert!(defaults.messages.direction.is_none() && defaults.messages.sort_by.is_none());
    let limit = recovery_query(
        Some(ListMessagesOptions {
            limit: Some(u32::MAX),
            ..Default::default()
        }),
        None,
        None,
    )?;
    assert_eq!(limit.messages.limit.unwrap() + 1, i64::from(u32::MAX) + 1);
    for status in [DeliveryStatus::Failed, DeliveryStatus::Unpublished] {
        for kind in [MessageKind::Application, MessageKind::MembershipChange] {
            let query = recovery_query(
                Some(ListMessagesOptions {
                    delivery_status: Some(status.clone()),
                    kind: Some(kind.clone()),
                    sort_by: Some(MessageSortBy::SentAt),
                    ..Default::default()
                }),
                None,
                None,
            )?;
            assert!(query.messages.kind.is_some() && query.messages.delivery_status.is_some());
        }
    }
    for options in [
        ListMessagesOptions {
            limit: Some(0),
            ..Default::default()
        },
        ListMessagesOptions {
            sort_by: Some(MessageSortBy::InsertedAt),
            ..Default::default()
        },
        ListMessagesOptions {
            delivery_status: Some(DeliveryStatus::Published),
            ..Default::default()
        },
    ] {
        assert!(matches!(
            recovery_query(Some(options), None, None),
            Err(XmtpError::InvalidArgument(_))
        ));
    }
    for token in [
        "",
        "dc1_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
        "mc1_!",
        "mc1_AA",
        "mc1_AAAAAAAAAAAAAAAAAAAAAA=",
    ] {
        assert!(matches!(
            recovery_query(
                None,
                Some(MessageRecoveryPosition {
                    sent_at: Timestamp(0),
                    message_cursor: token.into()
                }),
                None
            ),
            Err(XmtpError::InvalidCursor(_))
        ));
    }
}
