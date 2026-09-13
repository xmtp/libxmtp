use super::*;
use crate::GenericError;
use xmtp_common::assert_err;
use xmtp_mls::subscriptions::incoming::{
    IncomingConnection, IncomingError, IncomingProcessing, IncomingRegistration, IncomingStatus,
    IncomingTopicStatus,
};
use xmtp_proto::types::{Cursor, Topic};

#[xmtp_common::test(unwrap_try = true)]
fn replay_cursor_rejects_malformed_database_identity_with_typed_error() {
    for length in [0, 15, 17] {
        assert_err!(
            DeliveryCursor::try_from(FfiDeliveryCursor {
                database_id: vec![7; length],
                delivery_sequence: 1,
            }),
            FfiError::Error(GenericError::Storage(StorageError::Stream(
                StreamStorageError::ForeignCursor
            )))
        );
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn catch_up_translation_keeps_previous_generation_and_typed_blocked_cause() {
    let topic = Topic::new_group_message([3; 16]);
    let error = Arc::new(IncomingError::UnsupportedTopic);
    let previous = IncomingStatus {
        scope_generation: 8,
        connection_generation: 3,
        connection: IncomingConnection::Closed,
        topics: vec![],
        discovery_pending: false,
        processing: IncomingProcessing::Cancelled,
        error: None,
        previous: None,
    };
    let status = IncomingStatus {
        scope_generation: 9,
        connection_generation: 4,
        connection: IncomingConnection::Connected,
        topics: vec![IncomingTopicStatus {
            topic: topic.clone(),
            scope_generation: 9,
            registration: IncomingRegistration::Active,
            target: Some(Cursor(12)),
            received: Cursor(12),
            processed: Cursor(10),
            unresolved_welcomes: 2,
            processing: IncomingProcessing::Blocked,
            blocked: Some("unsupported_topic".to_owned()),
            error: Some(Arc::clone(&error)),
        }],
        discovery_pending: true,
        processing: IncomingProcessing::Blocked,
        error: Some(error),
        previous: Some(Box::new(previous)),
    };

    let snapshot = FfiMessageCatchUpSnapshot::from(status);
    let previous = snapshot.previous?;
    assert_eq!(previous.scope_generation, 8);
    assert_eq!(previous.connection_generation, 3);
    assert!(matches!(previous.connection, FfiMessageConnection::Closed));
    assert!(matches!(
        previous.processing,
        FfiMessageProcessing::Cancelled
    ));
    let current = snapshot.current;
    assert_eq!(current.scope_generation, 9);
    assert_eq!(current.connection_generation, 4);
    assert!(matches!(
        current.connection,
        FfiMessageConnection::Connected
    ));
    assert!(matches!(current.processing, FfiMessageProcessing::Blocked));
    assert!(current.discovery_pending);
    let cause = current.error?;
    assert_eq!(cause.code, "unsupported_topic");
    assert_eq!(cause.message, "unsupported incoming topic");
    assert!(!cause.retryable);
    assert_eq!(current.topics.len(), 1);
    let current_topic = current.topics.first()?;
    assert_eq!(current_topic.topic, topic.cloned_vec());
    assert_eq!(current_topic.scope_generation, 9);
    assert!(matches!(
        current_topic.registration,
        FfiMessageRegistration::Active
    ));
    assert_eq!(current_topic.target, Some(12));
    assert_eq!(current_topic.received, 12);
    assert_eq!(current_topic.processed, 10);
    assert_eq!(current_topic.unresolved_welcomes, 2);
    assert!(matches!(
        current_topic.processing,
        FfiMessageProcessing::Blocked
    ));
    assert_eq!(current_topic.blocked.as_deref(), Some("unsupported_topic"));
    assert_eq!(current_topic.error.as_ref()?.code, "unsupported_topic");
    assert!(!current_topic.error.as_ref()?.retryable);
}
