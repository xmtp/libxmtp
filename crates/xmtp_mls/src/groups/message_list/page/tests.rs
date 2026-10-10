use super::*;
use xmtp_db::group_message::{ContentType, DeliveryStatus, GroupMessageKind, RecoveryPosition};

#[xmtp_common::test(unwrap_try = true)]
fn recovery_visibility_filter_moves_raw_bound_buffers() {
    let query = RecoveryQueryArgs {
        messages: MsgQueryArgs {
            kind: Some(GroupMessageKind::MembershipChange),
            delivery_status: Some(DeliveryStatus::Failed),
            limit: Some(50),
            exclude_content_types: Some(vec![ContentType::Markdown, ContentType::Reaction]),
            ..Default::default()
        },
        before: Some(RecoveryPosition {
            sent_at_ns: -10,
            database_id: [1; 16],
            message_id: vec![3; 1024 * 1024],
        }),
        after: Some(RecoveryPosition {
            sent_at_ns: 20,
            database_id: [2; 16],
            message_id: vec![4; 1024 * 1024 + 1],
        }),
    };
    let buffers = [&query.before, &query.after].map(|position| {
        let position = position.as_ref().unwrap();
        (
            position.message_id.as_ptr(),
            position.message_id.capacity(),
            position.sent_at_ns,
            position.database_id,
            position.message_id.len(),
            position.message_id[0],
        )
    });
    let filtered = visible_recovery_query(query);
    for (position, expected) in [&filtered.before, &filtered.after].into_iter().zip(buffers) {
        let position = position.as_ref().unwrap();
        assert_eq!(position.message_id.as_ptr(), expected.0);
        assert_eq!(position.message_id.capacity(), expected.1);
        assert_eq!(position.sent_at_ns, expected.2);
        assert_eq!(position.database_id, expected.3);
        assert_eq!(position.message_id.len(), expected.4);
        assert_eq!(position.message_id[0], expected.5);
    }
    assert_eq!(
        filtered.messages.kind,
        Some(GroupMessageKind::MembershipChange)
    );
    assert_eq!(
        filtered.messages.delivery_status,
        Some(DeliveryStatus::Failed)
    );
    assert_eq!(filtered.messages.limit, Some(50));
    assert_eq!(
        filtered.messages.exclude_content_types,
        Some(vec![
            ContentType::Markdown,
            ContentType::Reaction,
            ContentType::ReadReceipt,
            ContentType::DeleteMessage,
        ])
    );
}
