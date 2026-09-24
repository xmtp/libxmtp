use std::{
    cell::Cell,
    collections::VecDeque,
    sync::{Arc, Weak},
};

use parking_lot::{Mutex, ReentrantMutex};
use tokio::sync::Notify;
use xmtp_common::{MaybeSend, MaybeSync};

use crate::{ClientEvent, EventFilter, EventKind, Lagged};

const UNBOUNDED_QUEUE_WARNING_DEPTH: usize = 10_000;

thread_local! {
    static FILTER_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct FilterGuard;

impl FilterGuard {
    fn enter() -> Self {
        FILTER_DEPTH.with(|depth| depth.set(depth.get() + 1));
        Self
    }
}

impl Drop for FilterGuard {
    fn drop(&mut self) {
        FILTER_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

/// Extra facts used by filters. A DM identifier is attached by the emitter.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventContext {
    pub dm_identifier: Option<Vec<u8>>,
    /// The emitter sets this only after checking the stored reference and sender.
    pub references_own_messages: bool,
}

/// One bus item. A write may carry a public event, an internal fact, or both.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventEnvelope<I> {
    pub client: Option<ClientEvent>,
    pub internal: Option<I>,
    pub context: EventContext,
}

impl<I> EventEnvelope<I> {
    pub fn new(client: Option<ClientEvent>, internal: Option<I>, context: EventContext) -> Self {
        Self {
            client,
            internal,
            context,
        }
    }
}

/// The only interface an emitter needs. A public-only crate uses an adapter.
pub trait EventWriter<I = ()>: MaybeSend + MaybeSync {
    fn emit(&self, client: Option<ClientEvent>, internal: Option<I>) {
        self.emit_with_context(client, internal, EventContext::default());
    }

    fn emit_with_context(
        &self,
        client: Option<ClientEvent>,
        internal: Option<I>,
        context: EventContext,
    );
}

struct BusInner<I> {
    subscriptions: Mutex<Vec<Weak<SubscriptionInner<I>>>>,
    dispatch_lock: ReentrantMutex<()>,
    buffer_lock: ReentrantMutex<()>,
}

/// One client's live event bus. The client owns this value and passes writers down.
pub struct EventBus<I> {
    inner: Arc<BusInner<I>>,
}

impl<I> Clone for EventBus<I> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<I> Default for EventBus<I> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I> EventBus<I> {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(BusInner {
                subscriptions: Mutex::new(Vec::new()),
                dispatch_lock: ReentrantMutex::new(()),
                buffer_lock: ReentrantMutex::new(()),
            }),
        }
    }

    /// Registers before returning. `None` gives an internal worker an unbounded queue.
    pub fn subscribe(&self, filter: EventFilter<I>, queue_depth: Option<usize>) -> Subscription<I> {
        self.assert_not_dispatching();
        let _dispatch = self.inner.dispatch_lock.lock();
        let inner = Arc::new(SubscriptionInner {
            filter,
            queue_depth,
            state: Mutex::new(QueueState::default()),
            changed: Notify::new(),
        });
        self.inner.subscriptions.lock().push(Arc::downgrade(&inner));
        Subscription { inner }
    }

    /// Runs one synchronous emitting write. Flush follows a successful commit only.
    pub fn with_buffer<R, E>(
        &self,
        f: impl FnOnce(&EventBuffer<'_, I>) -> Result<R, E>,
    ) -> Result<R, E>
    where
        I: Clone,
    {
        self.assert_not_dispatching();
        assert!(
            !self.inner.buffer_lock.is_owned_by_current_thread(),
            "nested event buffer on the same bus"
        );
        let _guard = self.inner.buffer_lock.lock();
        let buffer = EventBuffer {
            bus: self,
            pending: Mutex::new(Vec::new()),
        };
        let result = f(&buffer);
        if result.is_ok() {
            buffer.flush();
        } else {
            buffer.clear();
        }
        result
    }

    fn publish(&self, event: EventEnvelope<I>)
    where
        I: Clone,
    {
        if event.client.is_none() && event.internal.is_none() {
            return;
        }
        self.assert_not_dispatching();
        let _dispatch = self.inner.dispatch_lock.lock();
        if let Some(client) = &event.client {
            tracing::debug!(kind = client.kind().name(), "client event emitted");
        }
        let mut entries = self.inner.subscriptions.lock();
        entries.retain(|weak| {
            if let Some(subscription) = weak.upgrade() {
                subscription.deliver(&event);
                true
            } else {
                false
            }
        });
    }

    fn assert_not_dispatching(&self) {
        assert!(
            FILTER_DEPTH.with(|depth| depth.get() == 0),
            "an internal event filter must not call an event bus"
        );
        assert!(
            !self.inner.dispatch_lock.is_owned_by_current_thread(),
            "an internal event filter must not call its event bus"
        );
    }
}

impl<I> EventWriter<I> for EventBus<I>
where
    I: Clone + MaybeSend + MaybeSync + 'static,
{
    fn emit_with_context(
        &self,
        client: Option<ClientEvent>,
        internal: Option<I>,
        context: EventContext,
    ) {
        self.publish(EventEnvelope::new(client, internal, context));
    }
}

/// A transaction's pending events. Only `EventBus::with_buffer` can create it.
pub struct EventBuffer<'a, I> {
    bus: &'a EventBus<I>,
    pending: Mutex<Vec<EventEnvelope<I>>>,
}

impl<I: Clone> EventBuffer<'_, I> {
    /// Publish after the matching state write committed. Events of one write use kind-table order.
    fn flush(&self) {
        let mut pending = std::mem::take(&mut *self.pending.lock());
        pending.sort_by_key(|event| event.client.as_ref().map(|client| client.kind()));
        for event in pending {
            self.bus.publish(event);
        }
    }

    /// Discard after rollback.
    fn clear(&self) {
        self.pending.lock().clear();
    }

    /// Keep the outer write's events when one storage savepoint rolls back.
    pub fn savepoint<R, E>(&self, f: impl FnOnce(&Self) -> Result<R, E>) -> Result<R, E> {
        let checkpoint = self.pending.lock().len();
        let result = f(self);
        if result.is_err() {
            self.pending.lock().truncate(checkpoint);
        }
        result
    }
}

impl<I> EventWriter<I> for EventBuffer<'_, I>
where
    I: Clone + MaybeSend + MaybeSync + 'static,
{
    fn emit_with_context(
        &self,
        client: Option<ClientEvent>,
        internal: Option<I>,
        context: EventContext,
    ) {
        if client.is_some() || internal.is_some() {
            self.pending
                .lock()
                .push(EventEnvelope::new(client, internal, context));
        }
    }
}

/// A weak, public-only handle for lower crates and shared resources.
pub struct PublicBusWriter<I> {
    bus: Weak<BusInner<I>>,
}

impl<I> PublicBusWriter<I> {
    pub fn new(bus: &EventBus<I>) -> Self {
        Self {
            bus: Arc::downgrade(&bus.inner),
        }
    }
}

impl<I> EventWriter<()> for PublicBusWriter<I>
where
    I: Clone + MaybeSend + MaybeSync + 'static,
{
    fn emit_with_context(
        &self,
        client: Option<ClientEvent>,
        _internal: Option<()>,
        context: EventContext,
    ) {
        if let Some(inner) = self.bus.upgrade() {
            EventBus { inner }.publish(EventEnvelope::new(client, None, context));
        }
    }
}

/// Public-only view of a transaction buffer.
pub struct PublicBufferWriter<'b, 'bus, I> {
    buffer: &'b EventBuffer<'bus, I>,
}

impl<'b, 'bus, I> PublicBufferWriter<'b, 'bus, I> {
    pub fn new(buffer: &'b EventBuffer<'bus, I>) -> Self {
        Self { buffer }
    }
}

impl<I> EventWriter<()> for PublicBufferWriter<'_, '_, I>
where
    I: Clone + MaybeSend + MaybeSync + 'static,
{
    fn emit_with_context(
        &self,
        client: Option<ClientEvent>,
        _internal: Option<()>,
        context: EventContext,
    ) {
        self.buffer.emit_with_context(client, None, context);
    }
}

struct QueueState<I> {
    items: VecDeque<EventEnvelope<I>>,
    discarded: u64,
    lagged_after: usize,
    in_flight: usize,
    warned: bool,
    closed: bool,
}

impl<I> Default for QueueState<I> {
    fn default() -> Self {
        Self {
            items: VecDeque::new(),
            discarded: 0,
            lagged_after: 0,
            in_flight: 0,
            warned: false,
            closed: false,
        }
    }
}

struct SubscriptionInner<I> {
    filter: EventFilter<I>,
    queue_depth: Option<usize>,
    state: Mutex<QueueState<I>>,
    changed: Notify,
}

impl<I: Clone> SubscriptionInner<I> {
    fn deliver(&self, event: &EventEnvelope<I>) {
        let public = event
            .client
            .as_ref()
            .filter(|client| self.filter.matches_public(client, &event.context));
        let internal = event.internal.as_ref().filter(|internal| {
            let _filter_guard = FilterGuard::enter();
            self.filter.matches_internal(internal)
        });
        if public.is_none() && internal.is_none() {
            return;
        }
        let selected =
            EventEnvelope::new(public.cloned(), internal.cloned(), event.context.clone());
        let mut state = self.state.lock();
        if state.closed {
            return;
        }
        if self
            .queue_depth
            .is_some_and(|depth| state.items.len() + state.in_flight >= depth)
        {
            if state.discarded == 0 {
                state.lagged_after = state.items.len();
            }
            state.discarded = state.discarded.saturating_add(1);
        } else {
            state.items.push_back(selected);
            if self.queue_depth.is_none()
                && state.items.len() > UNBOUNDED_QUEUE_WARNING_DEPTH
                && !state.warned
            {
                state.warned = true;
                tracing::warn!(
                    depth = state.items.len(),
                    "unbounded event subscription queue is growing"
                );
            }
        }
        drop(state);
        self.changed.notify_one();
    }

    fn take(&self, lease: bool) -> Option<Option<EventEnvelope<I>>> {
        let mut state = self.state.lock();
        if state.closed {
            return Some(None);
        }
        if state.discarded > 0 && state.lagged_after == 0 {
            let discarded = std::mem::take(&mut state.discarded);
            let event = EventEnvelope::new(
                Some(ClientEvent::Lagged(Lagged { discarded })),
                None,
                EventContext::default(),
            );
            return Some(Some(event));
        }
        if let Some(event) = state.items.pop_front() {
            if state.lagged_after > 0 {
                state.lagged_after -= 1;
            }
            if lease {
                state.in_flight += 1;
            }
            return Some(Some(event));
        }
        None
    }
}

/// One independent, live subscription. Dropping it also closes it.
pub struct Subscription<I> {
    inner: Arc<SubscriptionInner<I>>,
}

impl<I> Subscription<I> {
    pub fn close(&self) {
        let mut state = self.inner.state.lock();
        state.closed = true;
        state.items.clear();
        state.discarded = 0;
        drop(state);
        self.inner.changed.notify_waiters();
    }

    pub fn is_closed(&self) -> bool {
        self.inner.state.lock().closed
    }
}

impl<I: Clone> Subscription<I> {
    /// Return the next item, or `None` after close. No runtime is needed to emit.
    pub async fn next(&self) -> Option<EventEnvelope<I>> {
        loop {
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(item) = self.inner.take(false) {
                return item;
            }
            notified.await;
        }
    }

    /// Keep a callback's item inside the queue bound until its returned value completes.
    pub async fn next_for_callback(&self) -> Option<EventLease<I>> {
        loop {
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(item) = self.inner.take(true) {
                return item.map(|event| {
                    let counted = event
                        .client
                        .as_ref()
                        .is_none_or(|client| client.kind() != EventKind::Lagged);
                    EventLease {
                        event,
                        subscription: self.inner.clone(),
                        counted,
                    }
                });
            }
            notified.await;
        }
    }

    /// Drain ready items after a worker wake. This includes a pending `lagged` item.
    pub fn drain(&self) -> Vec<EventEnvelope<I>> {
        let mut items = Vec::new();
        while let Some(Some(event)) = self.inner.take(false) {
            items.push(event);
        }
        items
    }
}

impl<I> Drop for Subscription<I> {
    fn drop(&mut self) {
        self.close();
    }
}

/// A callback handoff. Drop it after the callback's future completes.
pub struct EventLease<I> {
    pub event: EventEnvelope<I>,
    subscription: Arc<SubscriptionInner<I>>,
    counted: bool,
}

impl<I> Drop for EventLease<I> {
    fn drop(&mut self) {
        let mut state = self.subscription.state.lock();
        if self.counted {
            state.in_flight -= 1;
        }
        drop(state);
        self.subscription.changed.notify_one();
    }
}

#[cfg(test)]
mod tests;
