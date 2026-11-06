//! Live conversation notifications from committed group discovery.

use super::{Result, local_delivery::LocalDeliveryError};
use futures::Stream;
use std::{
    collections::{HashMap, VecDeque},
    pin::Pin,
    sync::{Arc, Weak},
    task::{Context, Poll},
};
use xmtp_common::{BoxDynStream, time::sleep};
use xmtp_db::{
    consent_record::ConsentState,
    group::{ConversationType, GroupQueryArgs, StoredGroup},
    prelude::*,
};
use xmtp_events::EventFilter;
use xmtp_mls_core::events::InternalEvent;
use xmtp_mls_engine::groups::{MlsGroup, context::XmtpSharedContext};
use xmtp_mls_engine::subscriptions::incoming::{
    IncomingCoordinator, IncomingLease, IncomingScope, stream_api::GroupSetGate,
};
use xmtp_proto::{
    api_client::XmtpMlsStreams,
    types::{GroupId, Topic},
};

const ALL_CONSENT_STATES: [ConsentState; 3] = [
    ConsentState::Allowed,
    ConsentState::Unknown,
    ConsentState::Denied,
];

/// Local notification history, not durable receipt or processing progress.
#[derive(Default)]
struct KnownConversations {
    /// A later Welcome for an existing group is a new rejoin notification.
    welcomes: HashMap<GroupId, Option<i64>>,
    /// Keep one selected group per participant pair for this stream's lifetime.
    dms: HashMap<String, GroupId>,
}

impl KnownConversations {
    /// Exclude groups already committed when the live stream starts.
    fn from_groups(groups: Vec<StoredGroup>) -> Self {
        let mut known = Self::default();
        for group in groups {
            known.welcomes.insert(group.id, group.sequence_id);
            if let Some(dm_id) = group.dm_id {
                known.dms.entry(dm_id).or_insert(group.id);
            }
        }
        known
    }

    /// Observe committed discovery without letting DM list ranking change its identity.
    fn observe(&mut self, group: &StoredGroup, include_duplicate_dms: bool) -> bool {
        if self.welcomes.insert(group.id, group.sequence_id) == Some(group.sequence_id)
            || ConversationType::virtual_types().contains(&group.conversation_type)
        {
            return false;
        }
        if !include_duplicate_dms && let Some(dm_id) = &group.dm_id {
            return *self.dms.entry(dm_id.clone()).or_insert(group.id) == group.id;
        }
        true
    }
}

/// The groups the stream selects, or none while the group-set generation
/// equals the one `gate` recorded at its last successful read. A new group, a
/// later Welcome and a consent change all change the generation. A message
/// does not, though it can make another group a stitched DM's winner: when
/// that DM's groups hold different consent, a consent-filtered stream reports
/// it at the next change of the group set instead of the next pass.
// implements: PROC-015
fn changed_groups<C: XmtpSharedContext>(
    context: &C,
    query: &GroupQueryArgs,
    gate: &mut GroupSetGate,
) -> std::result::Result<Vec<StoredGroup>, xmtp_db::ConnectionError> {
    let db = context.db();
    let generation = db.group_set_generation()?;
    if !gate.changed(generation) {
        return Ok(Vec::new());
    }
    let groups = if query.consent_states.as_ref().is_some_and(Vec::is_empty) {
        Vec::new()
    } else {
        #[cfg(any(test, feature = "test-hooks"))]
        test_hooks::count_groups_read();
        db.find_groups(query)?
    };
    gate.record(generation);
    Ok(groups)
}

/// Notify one subscriber of committed local creation and Welcome joins.
pub struct StreamConversations<C: XmtpSharedContext> {
    inner: BoxDynStream<'static, Result<MlsGroup<C>>>,
    lease: Weak<IncomingLease>,
}

impl<C: XmtpSharedContext + 'static> StreamConversations<C> {
    pub async fn new(
        context: &C,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self>
    where
        C::ApiClient: XmtpMlsStreams,
    {
        Self::new_owned(
            context.clone(),
            conversation_type,
            include_duplicate_dms,
            consent_states,
        )
        .await
    }

    /// Capture the local discovery baseline before starting the shared receiver.
    pub async fn new_owned(
        context: C,
        conversation_type: Option<ConversationType>,
        include_duplicate_dms: bool,
        consent_states: Option<Vec<ConsentState>>,
    ) -> Result<Self>
    where
        C::ApiClient: XmtpMlsStreams,
    {
        context.server_configuration().check()?;
        let events = context.events().subscribe(
            EventFilter::default().with_internal(|event| {
                matches!(event, InternalEvent::GroupJoined { is_sync: false, .. })
            }),
            Some(10),
        );
        let known = KnownConversations::from_groups(context.db().find_groups(GroupQueryArgs {
            include_sync_groups: true,
            include_duplicate_dms: true,
            consent_states: Some(ALL_CONSENT_STATES.to_vec()),
            ..Default::default()
        })?);
        let coordinator = IncomingCoordinator::for_context(&context);
        let lease = Arc::new(coordinator.acquire_stream(IncomingScope::Topics(vec![
            Topic::new_welcome_message(context.installation_id()),
        ])));
        let query = GroupQueryArgs {
            conversation_type,
            consent_states: Some(consent_states.unwrap_or_else(|| ALL_CONSENT_STATES.to_vec())),
            include_duplicate_dms,
            ..Default::default()
        };
        let observer = Arc::downgrade(&lease);
        let stream = futures::stream::unfold(
            Some((
                context,
                events,
                lease,
                known,
                VecDeque::new(),
                query,
                GroupSetGate::default(),
            )),
            |state| async move {
                let (context, events, lease, mut known, mut ready, query, mut gate) = state?;
                loop {
                    if let Err(error) = context.server_configuration().check() {
                        lease.close();
                        return Some((Err(error.into()), None));
                    }
                    if context.is_closed() {
                        return None;
                    }
                    if let Err(error) = lease.check_recovery() {
                        lease.close();
                        return Some((Err(LocalDeliveryError::from(error).into()), None));
                    }
                    if let Some(group) = ready.pop_front() {
                        return Some((
                            Ok(group),
                            Some((context, events, lease, known, ready, query, gate)),
                        ));
                    }
                    #[cfg(any(test, feature = "test-hooks"))]
                    test_hooks::count_pass();
                    match changed_groups(&context, &query, &mut gate) {
                        Ok(groups) => {
                            for group in groups {
                                if known.observe(&group, query.include_duplicate_dms) {
                                    ready.push_back(MlsGroup::new(
                                        context.clone(),
                                        group.id,
                                        group.dm_id,
                                        group.conversation_type,
                                        group.created_at_ns,
                                    ));
                                }
                            }
                        }
                        Err(error) => {
                            // The database operation has already used its normal
                            // retry policy. Only the app can start another stream.
                            lease.close();
                            return Some((Err(error.into()), None));
                        }
                    }
                    if !ready.is_empty() {
                        continue;
                    }
                    tokio::select! {
                        _ = context.cancellation_token().cancelled() => {},
                        _ = lease.changed() => {},
                        _ = sleep(context.stream_policy().active_database_poll_interval) => {},
                        event = events.next() => { event?; },
                    }
                }
            },
        );
        Ok(Self {
            inner: Box::pin(stream),
            lease: observer,
        })
    }

    /// Observe the same connection that supplies this conversation stream.
    pub fn lease(&self) -> Option<Arc<IncomingLease>> {
        self.lease.upgrade()
    }
}

impl<C: XmtpSharedContext> Stream for StreamConversations<C> {
    type Item = Result<MlsGroup<C>>;
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

#[cfg(any(test, feature = "test-hooks"))]
mod test_hooks;

#[cfg(test)]
mod test {
    use super::test_hooks::{GROUPS_READS, PASSES};
    use super::*;
    use futures::StreamExt;
    use std::cell::Cell;
    use xmtp_mls_engine::groups::test_support::{EnginePeer, EngineTestPeer};

    /// Idle passes do not reread the groups table, and an invitation that
    /// changes the group set is still delivered on the next pass.
    // verifies: CONS-044, PROC-015
    #[xmtp_common::test(unwrap_try = true)]
    async fn conversation_stream_rereads_groups_only_after_a_change() {
        let alix = EngineTestPeer::new().await;
        let bo = EngineTestPeer::new().await;
        let passes = || PASSES.with(Cell::get);
        let reads = || GROUPS_READS.with(Cell::get);
        let mut stream = StreamConversations::new(&bo.context, None, false, None).await?;
        tokio::select! {
            _ = stream.next() => panic!("the stream yielded with no new conversation"),
            waited = xmtp_common::wait_for_ge(|| async { passes() }, 3) => waited?,
        }
        assert_eq!(reads(), 1, "an idle pass reread the groups table");

        let group = alix.create_group(None, None)?;
        group.add_members(&[bo.context.inbox_id()]).await?;
        assert_eq!(stream.next().await.unwrap()?.group_id, group.group_id);
        assert_eq!(reads(), 2);
    }
}
