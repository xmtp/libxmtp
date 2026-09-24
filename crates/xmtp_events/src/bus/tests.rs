use std::{
    future::Future,
    sync::{Arc, Barrier, mpsc},
    task::{Context, Poll, Waker},
};

use super::*;
use crate::{
    ConsentChanged, ConsentEntityKind, ConsentState, ContentTypeId, ConversationJoined,
    ConversationRemoved, ConversationType, EventKind, GroupRef, HmacKeysUpdated, JoinOrigin,
    MessageReceived, MetadataChanged, RemovalCause,
};

fn joined(group: u8) -> ClientEvent {
    ClientEvent::ConversationJoined(ConversationJoined {
        group_id: vec![group],
        conversation_type: ConversationType::Group,
        origin: JoinOrigin::Created,
        adder_inbox_id: None,
    })
}

fn received(group: u8, id: u8, content_type: Option<ContentTypeId>) -> ClientEvent {
    ClientEvent::MessageReceived(MessageReceived {
        group_id: vec![group],
        message_id: vec![id],
        content_type,
        sender_inbox_id: "other".into(),
    })
}

fn public<I>(event: ClientEvent) -> EventEnvelope<I> {
    EventEnvelope::new(Some(event), None, EventContext::default())
}

#[xmtp_common::test(unwrap_try = true)]
async fn rollback_discards_events_and_releases_the_buffer() {
    let bus = EventBus::<u8>::new();
    let sub = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]),
        Some(1024),
    );
    let failed: Result<(), ()> = bus.with_buffer(|buffer| {
        buffer.emit(Some(joined(1)), Some(7));
        Err(())
    });
    assert!(failed.is_err());
    assert!(sub.drain().is_empty());
    bus.with_buffer(|buffer| {
        buffer.emit(Some(joined(2)), None);
        Ok::<_, ()>(())
    })?;
    assert_eq!(sub.drain(), vec![public(joined(2))]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_write_flushes_in_kind_table_order() {
    let bus = EventBus::<()>::new();
    let sub = bus.subscribe(EventFilter::new(EventKind::ALL), Some(1024));
    bus.with_buffer(|buffer| {
        buffer.emit(
            Some(ClientEvent::ConversationRemoved(ConversationRemoved {
                group_id: vec![1],
                cause: RemovalCause::Removed,
            })),
            None,
        );
        buffer.emit(
            Some(ClientEvent::ConversationMetadataChanged(MetadataChanged {
                group_id: vec![1],
                changed: vec!["name".into()],
            })),
            None,
        );
        buffer.emit(Some(joined(1)), None);
        Ok::<_, ()>(())
    })?;
    let kinds: Vec<_> = sub
        .drain()
        .into_iter()
        .map(|event| event.client.unwrap().kind())
        .collect();
    assert_eq!(
        kinds,
        [
            EventKind::ConversationJoined,
            EventKind::ConversationRemoved,
            EventKind::ConversationMetadataChanged,
        ]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_buffers_flush_in_commit_order_and_immediate_emits_do_not_wait() {
    let bus = EventBus::<()>::new();
    let sub = bus.subscribe(EventFilter::new(EventKind::ALL), Some(1024));
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let first_bus = &bus;
        let first = scope.spawn(move || {
            first_bus
                .with_buffer(|buffer| {
                    buffer.emit(Some(joined(1)), None);
                    entered_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    Ok::<_, ()>(())
                })
                .unwrap();
        });
        entered_rx.recv().unwrap();
        let second_bus = &bus;
        let second = scope.spawn(move || {
            second_bus
                .with_buffer(|buffer| {
                    buffer.emit(Some(joined(2)), None);
                    Ok::<_, ()>(())
                })
                .unwrap();
        });
        bus.emit(Some(ClientEvent::HmacKeysUpdated(HmacKeysUpdated)), None);
        release_tx.send(()).unwrap();
        first.join().unwrap();
        second.join().unwrap();
    });
    let events = sub.drain();
    assert_eq!(
        events[0].client,
        Some(ClientEvent::HmacKeysUpdated(HmacKeysUpdated))
    );
    assert_eq!(events[1].client, Some(joined(1)));
    assert_eq!(events[2].client, Some(joined(2)));
}

#[xmtp_common::test(unwrap_try = true)]
async fn nested_buffer_fails_before_it_can_deadlock() {
    let bus = EventBus::<()>::new();
    bus.with_buffer(|_| {
        let nested = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<(), ()> = bus.with_buffer(|_| Ok(()));
        }));
        assert!(nested.is_err());
        Ok::<_, ()>(())
    })
    .unwrap();
}

#[xmtp_common::test(unwrap_try = true)]
async fn failed_savepoint_discards_only_its_own_events() {
    let bus = EventBus::<()>::new();
    let sub = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]),
        Some(1024),
    );
    bus.with_buffer(|buffer| {
        buffer.emit(Some(joined(1)), None);
        let failed: Result<(), ()> = buffer.savepoint(|savepoint| {
            savepoint.emit(Some(joined(2)), None);
            Err(())
        });
        assert!(failed.is_err());
        buffer.emit(Some(joined(3)), None);
        Ok::<_, ()>(())
    })?;
    assert_eq!(sub.drain(), vec![public(joined(1)), public(joined(3))]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_subscription_is_live_only() {
    let bus = EventBus::<()>::new();
    bus.emit(Some(joined(1)), None);
    let sub = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]),
        Some(1024),
    );
    bus.emit(Some(joined(2)), None);
    assert_eq!(sub.drain(), vec![public(joined(2))]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn bounded_queue_reports_exact_discards_before_later_events() {
    let bus = EventBus::<()>::new();
    let sub = bus.subscribe(EventFilter::new([EventKind::ConversationJoined]), Some(2));
    bus.emit(Some(joined(1)), None);
    bus.emit(Some(joined(2)), None);
    bus.emit(Some(joined(3)), None);
    bus.emit(Some(joined(4)), None);
    assert_eq!(sub.next().await, Some(public(joined(1))));
    bus.emit(Some(joined(5)), None);
    let events = sub.drain();
    assert_eq!(
        events,
        vec![
            public(joined(2)),
            public(ClientEvent::Lagged(Lagged { discarded: 2 })),
            public(joined(5)),
        ]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn unbounded_and_independent_queues_do_not_lose_events() {
    let bus = EventBus::<()>::new();
    let slow = bus.subscribe(EventFilter::new([EventKind::ConversationJoined]), Some(1));
    let fast = bus.subscribe(EventFilter::new([EventKind::ConversationJoined]), None);
    for id in 0..100 {
        bus.emit(Some(joined(id)), None);
    }
    assert_eq!(fast.drain().len(), 100);
    assert_eq!(
        slow.drain(),
        vec![
            public(joined(0)),
            public(ClientEvent::Lagged(Lagged { discarded: 99 })),
        ]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_reentrant_internal_filter_fails_and_the_bus_recovers() {
    let bus = EventBus::<u8>::new();
    let reentrant_writer = bus.clone();
    let app = bus.subscribe(EventFilter::new([EventKind::ConversationJoined]), Some(2));
    let worker = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]).with_internal(move |_| {
            reentrant_writer.emit(Some(joined(9)), None);
            true
        }),
        None,
    );

    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bus.emit(Some(joined(1)), Some(7));
    }));
    assert!(failed.is_err());
    bus.emit(Some(joined(2)), None);
    assert_eq!(app.drain(), vec![public(joined(1)), public(joined(2))]);
    assert_eq!(worker.drain(), vec![public(joined(2))]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_filter_cannot_start_a_buffered_write() {
    let bus = EventBus::<u8>::new();
    let reentrant_writer = bus.clone();
    let sub = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]).with_internal(move |_| {
            let _: Result<(), ()> = reentrant_writer.with_buffer(|_| Ok(()));
            true
        }),
        Some(2),
    );
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        bus.emit(Some(joined(1)), Some(7));
    }));
    assert!(failed.is_err());
    bus.emit(Some(joined(2)), None);
    assert_eq!(sub.drain(), vec![public(joined(2))]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_filter_cannot_call_another_bus() {
    let first = EventBus::<u8>::new();
    let second = EventBus::<u8>::new();
    let second_writer = second.clone();
    let first_sub = first.subscribe(
        EventFilter::new([EventKind::ConversationJoined]).with_internal(move |_| {
            second_writer.emit(Some(joined(9)), None);
            true
        }),
        Some(2),
    );
    let second_sub = second.subscribe(EventFilter::new(EventKind::ALL), Some(2));
    let failed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        first.emit(Some(joined(1)), Some(7));
    }));
    assert!(failed.is_err());
    assert!(second_sub.drain().is_empty());
    first.emit(Some(joined(2)), None);
    assert_eq!(first_sub.drain(), vec![public(joined(2))]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_publishers_keep_the_first_emit_first() {
    let bus = EventBus::<u8>::new();
    let entered = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let filter_entered = entered.clone();
    let filter_release = release.clone();
    let sub = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]).with_internal(move |_| {
            filter_entered.wait();
            filter_release.wait();
            true
        }),
        None,
    );
    std::thread::scope(|scope| {
        let first = scope.spawn(|| bus.emit(Some(joined(1)), Some(7)));
        entered.wait();
        let second = scope.spawn(|| bus.emit(Some(joined(2)), None));
        release.wait();
        first.join().unwrap();
        second.join().unwrap();
    });
    let events = sub.drain();
    assert_eq!(events[0].client, Some(joined(1)));
    assert_eq!(events[1].client, Some(joined(2)));
}

#[xmtp_common::test(unwrap_try = true)]
async fn concurrent_reads_each_receive_an_available_event() {
    let bus = EventBus::<()>::new();
    let sub = bus.subscribe(EventFilter::new([EventKind::ConversationJoined]), Some(2));
    let first = sub.next();
    let second = sub.next_for_callback();
    tokio::pin!(first, second);
    let mut context = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut context).is_pending());
    assert!(second.as_mut().poll(&mut context).is_pending());

    bus.emit(Some(joined(1)), None);
    bus.emit(Some(joined(2)), None);
    assert_eq!(
        first.as_mut().poll(&mut context),
        Poll::Ready(Some(public(joined(1))))
    );
    let Poll::Ready(Some(lease)) = second.as_mut().poll(&mut context) else {
        panic!("the second read did not receive the second event");
    };
    assert_eq!(lease.event, public(joined(2)));
}

#[xmtp_common::test(unwrap_try = true)]
async fn callback_lease_counts_toward_the_queue_depth() {
    let bus = EventBus::<()>::new();
    let sub = bus.subscribe(EventFilter::new([EventKind::ConversationJoined]), Some(1));
    bus.emit(Some(joined(1)), None);
    let lease = sub.next_for_callback().await.unwrap();
    bus.emit(Some(joined(2)), None);
    drop(lease);
    assert_eq!(
        sub.drain(),
        vec![public(ClientEvent::Lagged(Lagged { discarded: 1 }))]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn filter_matches_stitched_dm_second_group_and_conversation_consent() {
    let bus = EventBus::<()>::new();
    let mut filter = EventFilter::new([EventKind::ConversationJoined, EventKind::ConsentChanged]);
    filter.group_ids = Some(vec![vec![1]]);
    filter.dm_identifiers = vec![vec![9]];
    let sub = bus.subscribe(filter, Some(1024));
    bus.emit_with_context(
        Some(joined(2)),
        None,
        EventContext {
            dm_identifier: Some(vec![9]),
            ..Default::default()
        },
    );
    bus.emit(Some(joined(3)), None);
    bus.emit(
        Some(ClientEvent::ConsentChanged(ConsentChanged {
            entity_kind: ConsentEntityKind::Conversation,
            entity: "01".into(),
            state: ConsentState::Allowed,
        })),
        None,
    );
    assert_eq!(sub.drain().len(), 2);
}

#[xmtp_common::test(unwrap_try = true)]
async fn content_type_and_own_reference_filters_apply_only_to_received_messages() {
    let bus = EventBus::<()>::new();
    let ty = ContentTypeId {
        authority_id: "xmtp.org".into(),
        type_id: "reply".into(),
        version_major: 1,
    };
    let mut filter = EventFilter::new([
        EventKind::MessageReceived,
        EventKind::ConversationForkDetected,
    ]);
    filter.content_types = Some(vec![ty.clone()]);
    filter.references_own_messages = true;
    let sub = bus.subscribe(filter, Some(1024));
    bus.emit(Some(received(1, 1, Some(ty.clone()))), None);
    bus.emit_with_context(
        Some(received(1, 2, None)),
        None,
        EventContext {
            references_own_messages: true,
            ..Default::default()
        },
    );
    bus.emit_with_context(
        Some(received(1, 3, Some(ty))),
        None,
        EventContext {
            references_own_messages: true,
            ..Default::default()
        },
    );
    bus.emit(
        Some(ClientEvent::ConversationForkDetected(GroupRef {
            group_id: vec![1],
        })),
        None,
    );
    let events = sub.drain();
    assert_eq!(events.len(), 2);
    assert_eq!(
        events[0].client.as_ref().unwrap().kind(),
        EventKind::MessageReceived
    );
    assert_eq!(
        events[1].client.as_ref().unwrap().kind(),
        EventKind::ConversationForkDetected
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn own_reference_filter_requires_a_reply_or_current_reaction() {
    let bus = EventBus::<()>::new();
    let mut filter = EventFilter::new([EventKind::MessageReceived]);
    filter.references_own_messages = true;
    let sub = bus.subscribe(filter, Some(1024));
    for (id, type_id, version_major) in [
        (1, "reply", 1),
        (2, "reaction", 2),
        (3, "reaction", 1),
        (4, "text", 1),
    ] {
        bus.emit_with_context(
            Some(received(
                1,
                id,
                Some(ContentTypeId {
                    authority_id: "xmtp.org".into(),
                    type_id: type_id.into(),
                    version_major,
                }),
            )),
            None,
            EventContext {
                references_own_messages: true,
                ..Default::default()
            },
        );
    }
    let ids: Vec<_> = sub
        .drain()
        .into_iter()
        .map(|item| match item.client.unwrap() {
            ClientEvent::MessageReceived(message) => message.message_id[0],
            other => panic!("unexpected event: {other:?}"),
        })
        .collect();
    assert_eq!(ids, [1, 2]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn empty_filters_exclude_grouped_events_but_not_ungrouped_events() {
    let bus = EventBus::<()>::new();
    let none = bus.subscribe(EventFilter::new([]), Some(1024));
    let mut filter = EventFilter::new([
        EventKind::ConversationJoined,
        EventKind::MessageReceived,
        EventKind::HmacKeysUpdated,
    ]);
    filter.group_ids = Some(Vec::new());
    filter.content_types = Some(Vec::new());
    let ungrouped = bus.subscribe(filter, Some(1024));
    bus.emit(Some(joined(1)), None);
    bus.emit(Some(received(1, 2, None)), None);
    bus.emit(Some(ClientEvent::HmacKeysUpdated(HmacKeysUpdated)), None);
    assert!(none.drain().is_empty());
    assert_eq!(
        ungrouped.drain(),
        vec![public(ClientEvent::HmacKeysUpdated(HmacKeysUpdated))]
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn internal_and_public_sides_select_independently() {
    let bus = EventBus::<u8>::new();
    let worker = bus.subscribe(
        EventFilter::default().with_internal(|value| *value == 7),
        None,
    );
    let app = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]),
        Some(1024),
    );
    bus.emit(Some(joined(1)), Some(7));
    assert_eq!(worker.drain()[0].internal, Some(7));
    assert!(worker.drain().is_empty());
    assert_eq!(app.drain(), vec![public(joined(1))]);
}

#[xmtp_common::test(unwrap_try = true)]
async fn public_only_adapters_do_not_own_the_bus() {
    let bus = EventBus::<u8>::new();
    let sub = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]),
        Some(1024),
    );
    let writer = PublicBusWriter::new(&bus);
    writer.emit(Some(joined(1)), None);
    bus.with_buffer(|buffer| {
        PublicBufferWriter::new(buffer).emit(Some(joined(2)), None);
        Ok::<_, ()>(())
    })?;
    assert_eq!(sub.drain(), vec![public(joined(1)), public(joined(2))]);
    drop(bus);
    writer.emit(Some(joined(3)), None);
    assert!(sub.drain().is_empty());
}

#[xmtp_common::test(unwrap_try = true)]
async fn rust_kinds_equal_the_approved_spec_kinds() {
    // Names and order from the approved EVENT kind table.
    let expected = [
        "conversation.joined",
        "conversation.removed",
        "conversation.membership_changed",
        "conversation.metadata_changed",
        "conversation.paused",
        "message.received",
        "message.status_changed",
        "message.deleted",
        "message.expired",
        "consent.changed",
        "hmac_keys.updated",
        "identity.registered",
        "identity.own_installation_added",
        "identity.own_installation_revoked",
        "client.rejected_by_server",
        "client.lockout_changed",
        "conversation.fork_detected",
        "notifications.failed",
        "archive.restored",
        "connection.state_changed",
        "lagged",
    ];
    let actual: Vec<_> = EventKind::ALL.into_iter().map(EventKind::name).collect();
    assert_eq!(actual, expected);
}

#[xmtp_common::test(unwrap_try = true)]
async fn close_ends_reads_and_clears_queued_events() {
    let bus = EventBus::<()>::new();
    let sub = bus.subscribe(
        EventFilter::new([EventKind::ConversationJoined]),
        Some(1024),
    );
    bus.emit(Some(joined(1)), None);
    sub.close();
    assert_eq!(sub.next().await, None);
    bus.emit(Some(joined(2)), None);
    assert!(sub.drain().is_empty());
}
