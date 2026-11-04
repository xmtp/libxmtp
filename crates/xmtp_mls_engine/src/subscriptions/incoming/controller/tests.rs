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
