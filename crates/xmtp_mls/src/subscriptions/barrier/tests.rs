use super::*;
use crate::tester;
use prost::Message;
use xmtp_db::incoming_envelope::{IncomingRetry, NewIncomingEnvelope};
use xmtp_proto::backend_v1 as wire;

const TEST_TIMEOUT: Duration = Duration::from_secs(2);

std::thread_local! {
    static BEFORE_PROGRESS_READ: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn before_progress_read() {
    let hook = BEFORE_PROGRESS_READ.with(|slot| slot.borrow_mut().take());
    if let Some(hook) = hook {
        hook();
    }
}

struct ProgressReadHook(std::marker::PhantomData<std::rc::Rc<()>>);

impl Drop for ProgressReadHook {
    fn drop(&mut self) {
        BEFORE_PROGRESS_READ.with(|slot| slot.borrow_mut().take());
    }
}

fn on_next_progress_read(hook: impl FnOnce() + 'static) -> ProgressReadHook {
    assert!(
        BEFORE_PROGRESS_READ
            .with(|slot| slot.replace(Some(Box::new(hook))))
            .is_none()
    );
    ProgressReadHook(std::marker::PhantomData)
}

fn admit_pending<C: XmtpSharedContext>(
    context: &C,
    topic: &Topic,
    sequence: Cursor,
    payload: wire::client_envelope::Payload,
    defer: bool,
) -> Result<(), StorageError> {
    let kind = match topic.kind() {
        TopicKind::GroupMessagesV1 => NetworkEntityKind::Group,
        TopicKind::WelcomeMessagesV1 => NetworkEntityKind::Welcome,
        _ => unreachable!("only group and Welcome fixtures are used"),
    };
    let key = StreamTopic {
        entity_id: topic.identifier().to_vec(),
        kind,
    };
    let envelope = wire::ServerEnvelope {
        meta: Some(wire::EnvelopeMeta {
            cursor: Some(wire::Cursor {
                sequence_id: sequence.0,
            }),
            server_ns: xmtp_common::time::now_ns() as u64,
            topic: Some(wire::Topic {
                topic: topic.cloned_vec(),
            }),
            message_hash: Some(wire::MessageHash {
                hash: Some(wire::message_hash::Hash::Sha256(vec![7; 32])),
            }),
            ..Default::default()
        }),
        envelope: Some(wire::ClientEnvelope {
            payload: Some(payload),
        }),
    };
    context.db().admit_ordered_batch(
        &key,
        Cursor(0),
        &[NewIncomingEnvelope {
            sequence_id: sequence,
            envelope: envelope.encode_to_vec(),
        }],
        context.incoming_runtime().policy().incoming_limits(kind),
    )?;
    if defer {
        context.db().set_incoming_retry(
            &key,
            sequence,
            &IncomingRetry {
                retry_at_ns: xmtp_common::time::now_ns() + xmtp_common::NS_IN_MIN,
                blocked: false,
                error_code: Some("dependency_retry".into()),
                retry_expires_at_ns: None,
            },
        )?;
    }
    Ok(())
}

#[xmtp_common::test(unwrap_try = true)]
async fn fixed_welcome_discovery_excludes_later_scope_and_keeps_rejoined_groups() {
    tester!(alix, disable_workers);
    let initial = alix.create_group(None, None)?;
    let mut groups = HashSet::from([initial.group_id]);
    let welcome_topic = Topic::new_welcome_message(alix.context.installation_id());
    let welcome_target = Cursor(10);
    admit_pending(
        &alix.context,
        &welcome_topic,
        welcome_target,
        wire::client_envelope::Payload::WelcomeMessage(wire::WelcomeMessage::default()),
        true,
    )?;
    let targets = [
        (Topic::new_group_message(initial.group_id), Cursor(0)),
        (welcome_topic.clone(), welcome_target),
    ]
    .into();

    let local_id = alix.create_group(None, None)?.group_id;
    let discovered_id = alix.create_group(None, None)?.group_id;
    let later_id = alix.create_group(None, None)?.group_id;
    let context = alix.context.clone();
    // Complete Welcome installation at the old gap between discovery and progress.
    let _hook = on_next_progress_read(move || {
        crate::state_tx::state_write(context.mls_storage(), |tx| {
            let storage = tx.storage();
            let db = storage.db();
            db.record_welcome_discovery(discovered_id, welcome_target)?;
            let mut rejoined = db
                .find_groups(GroupQueryArgs {
                    include_sync_groups: true,
                    include_duplicate_dms: true,
                    ..Default::default()
                })?
                .into_iter()
                .find(|group| group.id == discovered_id)
                .unwrap();
            rejoined.sequence_id = Some(100);
            db.insert_or_replace_group(rejoined)?;
            db.record_welcome_discovery(discovered_id, Cursor(100))?;
            db.record_welcome_discovery(later_id, Cursor(20))?;
            db.complete_pending_envelope(
                &StreamTopic {
                    entity_id: context.installation_id().to_vec(),
                    kind: NetworkEntityKind::Welcome,
                },
                welcome_target,
            )?;
            Ok::<_, StorageError>(xmtp_db::TransactionOutcome::Continue(()))
        })
        .unwrap();
    });
    let snapshot = wait_for_targets(
        &alix.context,
        targets,
        Vec::new(),
        Some(WelcomeDiscovery {
            topic: welcome_topic.clone(),
            target: welcome_target,
            consent_states: None,
        }),
        IncomingReceivePolicy::ImmediateQuery,
        Instant::now() + TEST_TIMEOUT,
        &mut groups,
    )
    .await?;

    assert_eq!(groups, HashSet::from([initial.group_id, discovered_id]));
    let topics: HashSet<_> = snapshot
        .topics
        .iter()
        .map(|status| status.topic.clone())
        .collect();
    assert_eq!(
        topics,
        HashSet::from([
            Topic::new_group_message(initial.group_id),
            Topic::new_group_message(discovered_id),
            welcome_topic,
        ])
    );
    assert!(!topics.contains(&Topic::new_group_message(later_id)));
    assert!(!topics.contains(&Topic::new_group_message(local_id)));
    assert!(snapshot.topics.iter().all(BarrierTopic::complete));
}

#[xmtp_common::test(unwrap_try = true)]
async fn a_stalled_welcome_does_not_hold_known_group_processing() {
    tester!(alix, disable_workers);
    let group = alix.create_group(None, None)?;
    let uncaptured = alix.create_group(None, None)?;
    let group_topic = Topic::new_group_message(group.group_id);
    let uncaptured_topic = Topic::new_group_message(uncaptured.group_id);
    let welcome_topic = Topic::new_welcome_message(alix.context.installation_id());
    admit_pending(
        &alix.context,
        &group_topic,
        Cursor(10),
        wire::client_envelope::Payload::GroupMessage(wire::GroupMessage {
            data: vec![0, 1, 0],
            ..Default::default()
        }),
        false,
    )?;
    admit_pending(
        &alix.context,
        &welcome_topic,
        Cursor(20),
        wire::client_envelope::Payload::WelcomeMessage(wire::WelcomeMessage::default()),
        true,
    )?;
    let mut capture_failure = read_topic(&alix.context, &uncaptured_topic, Cursor(0));
    capture_failure.target = None;
    capture_failure.cause = Some(BarrierCause::TargetPending);
    let mut groups = HashSet::from([group.group_id, uncaptured.group_id]);
    let error = wait_for_targets(
        &alix.context,
        [
            (group_topic, Cursor(10)),
            (welcome_topic.clone(), Cursor(20)),
        ]
        .into(),
        vec![capture_failure],
        Some(WelcomeDiscovery {
            topic: welcome_topic.clone(),
            target: Cursor(20),
            consent_states: None,
        }),
        IncomingReceivePolicy::ImmediateQuery,
        Instant::now() + TEST_TIMEOUT,
        &mut groups,
    )
    .await
    .unwrap_err();

    let db = alix.context.db();
    let key = StreamTopic::group(group.group_id);
    assert_eq!(db.topic_progress(&key)?.processed, Cursor(10));
    assert_eq!(
        db.read_last_rejection(&key)?.unwrap().sequence_id,
        Cursor(10)
    );
    let BarrierError::Incomplete { reason, unfinished } = error;
    assert_eq!(reason, BarrierFailure::Deadline);
    assert_eq!(unfinished.len(), 2);
    let welcome = unfinished
        .iter()
        .find(|status| status.topic == welcome_topic)?;
    assert_eq!(welcome.target, Some(Cursor(20)));
    assert_eq!(welcome.received, Cursor(20));
    assert_eq!(welcome.unresolved_welcomes, vec![Cursor(20)]);
    let uncaptured = unfinished
        .iter()
        .find(|status| status.topic == uncaptured_topic)?;
    assert_eq!(uncaptured.target, None);
    assert!(matches!(
        uncaptured.cause,
        Some(BarrierCause::TargetPending)
    ));
}

#[xmtp_common::test(unwrap_try = true)]
async fn target_capture_timeout_reports_every_starting_topic() {
    tester!(alix, disable_workers);
    let first = alix.create_group(None, None)?;
    let second = alix.create_group(None, None)?;
    let topics = vec![
        Topic::new_group_message(first.group_id),
        Topic::new_group_message(second.group_id),
        Topic::new_welcome_message(alix.context.installation_id()),
    ];
    let error = receive_through_current_until(
        &alix.context,
        topics.clone(),
        Instant::now() - Duration::from_millis(1),
    )
    .await
    .unwrap_err();
    let BarrierError::Incomplete { reason, unfinished } = error;
    assert_eq!(reason, BarrierFailure::Deadline);
    assert_eq!(unfinished.len(), topics.len());
    assert_eq!(
        unfinished
            .iter()
            .map(|status| status.topic.clone())
            .collect::<HashSet<_>>(),
        topics.into_iter().collect()
    );
    for status in unfinished {
        assert_eq!(status.target, None);
        assert_eq!(status.received, Cursor(0));
        assert_eq!(status.processed, Cursor(0));
        assert!(matches!(status.cause, Some(BarrierCause::TargetPending)));
    }
}
