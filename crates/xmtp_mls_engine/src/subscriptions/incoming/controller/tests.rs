use super::*;
use crate::prelude::*;
use prost::Message;
use xmtp_proto::types::ReceivedEnvelope;

use crate::groups::{
    test_support::{
        EnginePeer, EngineTestContext, EngineTestPeer, EngineTestPeerOps, MockEngineContext,
    },
    welcome_sync::{HeldReadd, WelcomeService, held_readd},
};
use xmtp_common::Generate;
use xmtp_db::group::{ConversationType, GroupQueryArgs};
use xmtp_proto::{
    backend_v1 as wire,
    types::{GroupId, OrderedEnvelopeBatch},
};

mod barriers;
mod group_selection;
mod held_heads;
mod interest;
mod passes;
mod permanent_failures;
mod proof_dependencies;
mod rate_limits;
mod receipt;
mod rejected_queries;
mod rejected_targets;
mod welcome_prefixes;

fn controller<C: XmtpSharedContext + 'static>(context: C) -> Controller<C> {
    let (_, receiver) = mpsc::unbounded_channel();
    Controller::new(context, receiver, Arc::new(SharedState::default()))
}

fn identity_key(requirement: &IdentityRequirement) -> DependencyKey {
    DependencyKey::Identity(BTreeSet::from([requirement.clone()]))
}

fn coordinated_controller<C: XmtpSharedContext + 'static>(
    context: C,
) -> (Arc<IncomingCoordinator>, Controller<C>) {
    let (commands, receiver) = mpsc::unbounded_channel();
    let state = Arc::new(SharedState::default());
    state
        .connection_states
        .set_writer(Arc::new(xmtp_events::PublicBusWriter::new(
            context.events(),
        )));
    let coordinator = Arc::new(IncomingCoordinator {
        commands,
        generations: AtomicU64::new(0),
        state: state.clone(),
    });
    (coordinator, Controller::new(context, receiver, state))
}

fn add_scope<C: XmtpSharedContext + 'static>(
    controller: &mut Controller<C>,
    id: u64,
    topic: &Topic,
) {
    controller
        .state
        .statuses
        .lock()
        .insert(id, IncomingStatus::pending(id));
    controller.command(Command::Acquire {
        id,
        scope: IncomingScope::Topics(vec![topic.clone()]),
    });
    controller
        .scopes
        .get_mut(&id)
        .unwrap()
        .topics
        .insert(topic.clone());
}

fn context() -> MockEngineContext {
    xmtp_mls_core::context::test_support::mock_context()
}

fn add_barrier_scope<C: XmtpSharedContext + 'static>(
    controller: &mut Controller<C>,
    id: u64,
    topic: &Topic,
    target: Cursor,
    deadline: Instant,
) {
    controller
        .state
        .statuses
        .lock()
        .insert(id, IncomingStatus::pending(id));
    controller.command(Command::Acquire {
        id,
        scope: IncomingScope::Barrier {
            targets: [(topic.clone(), target)].into(),
            deadline,
            receive_policy: IncomingReceivePolicy::StreamFirst,
        },
    });
}

fn meta(topic: &Topic, sequence_id: u64) -> wire::EnvelopeMeta {
    wire::EnvelopeMeta {
        cursor: Some(wire::Cursor { sequence_id }),
        server_ns: 123,
        topic: Some(wire::Topic {
            topic: topic.cloned_vec(),
        }),
        message_hash: Some(wire::MessageHash {
            hash: Some(wire::message_hash::Hash::Sha256(vec![7; 32])),
        }),
        ..Default::default()
    }
}

struct SuspendedFactory;

impl SubscriptionFactory for SuspendedFactory {
    fn open(&self, _: TopicCursor, _: IncomingBatchLimits) -> SubscriptionFuture {
        panic!("the read-policy test must not open a transport");
    }

    fn is_suspended(&self) -> bool {
        true
    }
}

struct PendingFactory;

impl SubscriptionFactory for PendingFactory {
    fn open(&self, _: TopicCursor, _: IncomingBatchLimits) -> SubscriptionFuture {
        Box::pin(futures::future::pending())
    }
}

fn run_raw_sql<C: XmtpSharedContext>(context: &C, sql: &str) {
    use xmtp_db::{ConnectionExt, diesel::RunQueryDsl};
    context
        .db()
        .raw_query(|conn| xmtp_db::diesel::sql_query(sql).execute(conn))
        .unwrap();
}

/// One run-loop pass up to opening network interest.
fn pass<C: XmtpSharedContext + 'static>(controller: &mut Controller<C>) {
    controller.storage_error = controller.reconcile().err().map(Arc::new);
    controller.start_open();
}

/// Make `controller` a healthy stream registered for every topic it wants, at
/// target zero, so a pass starts no network work.
fn register_all<C: XmtpSharedContext + 'static>(controller: &mut Controller<C>) {
    while let Ok(command) = controller.commands.try_recv() {
        controller.command(command);
    }
    controller.reconcile().unwrap();
    let topics = controller.interested();
    controller.transport.requested = topics.clone();
    controller.transport.state = TransportState::Streaming(IncomingSubscription::new(
        Box::pin(futures::stream::pending()),
        |_| {},
    ));
    controller.registered(topics.into_iter().map(|topic| (topic, Cursor(0))).collect());
}

/// Store group rows without MLS state. Their heads take processing turns,
/// but processing them fails.
fn stored_groups<C: XmtpSharedContext>(context: &C, count: usize) -> Vec<Topic> {
    use xmtp_db::{Store, group::StoredGroup};
    (0..count)
        .map(|_| {
            let group = StoredGroup::builder()
                .id(GroupId::generate())
                .created_at_ns(0)
                .membership_state(GroupMembershipState::Allowed)
                .added_by_inbox_id("peer")
                .build()
                .unwrap();
            group.store(&context.db()).unwrap();
            Topic::new_group_message(group.id)
        })
        .collect()
}

/// Create local groups with MLS state, so processing can reject and complete
/// their malformed heads in order.
fn local_groups<C: XmtpSharedContext>(context: &C, count: usize) -> Vec<Topic> {
    use crate::groups::group_permissions::PolicySet;
    use xmtp_mls_group_state::group::GroupMetadataOptions;
    (0..count)
        .map(|_| {
            let group = MlsGroup::insert(
                context,
                None,
                GroupMembershipState::Allowed,
                ConversationType::Group,
                PolicySet::default(),
                GroupMetadataOptions::default(),
                None,
                false,
            )
            .unwrap();
            Topic::new_group_message(group.id)
        })
        .collect()
}

/// An envelope without a payload, which processing rejects as malformed.
fn empty_envelope(topic: &Topic, sequence: u64) -> ReceivedEnvelope {
    wire::ServerEnvelope {
        meta: Some(meta(topic, sequence)),
        envelope: Some(wire::ClientEnvelope::default()),
    }
    .into()
}

fn admit_empty<C: XmtpSharedContext + 'static>(
    controller: &mut Controller<C>,
    topic: &Topic,
    sequences: &[u64],
) {
    controller
        .admit_received_batch(OrderedEnvelopeBatch {
            topic: topic.clone(),
            after: Cursor(0),
            envelopes: sequences
                .iter()
                .map(|sequence| empty_envelope(topic, *sequence))
                .collect(),
        })
        .unwrap();
}

/// Set the retry state of the envelope at `sequence`.
fn retry_at<C: XmtpSharedContext>(
    context: &C,
    topic: &Topic,
    sequence: u64,
    retry_at_ns: i64,
    blocked: bool,
) {
    let retry = xmtp_db::incoming_envelope::IncomingRetry {
        retry_at_ns,
        blocked,
        error_code: blocked.then(|| "held".into()),
        retry_expires_at_ns: None,
    };
    assert!(
        context
            .db()
            .set_incoming_retry(&topic_key(topic).unwrap(), Cursor(sequence), &retry)
            .unwrap()
    );
}
