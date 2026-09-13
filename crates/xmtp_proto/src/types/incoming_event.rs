use super::{Cursor, Topic, TopicCursor};
use crate::backend_v1::ServerEnvelope;
use futures::StreamExt;
use xmtp_common::{BoxDynStream, MaybeSend, MaybeSync};

/// Limits for one complete transport delivery before payload processing.
#[derive(Clone, Copy, Debug)]
pub struct IncomingBatchLimits {
    /// Maximum number of envelopes across all topics in the delivery.
    pub max_rows: usize,
    /// Maximum sum of encoded `ServerEnvelope` sizes in the delivery.
    pub max_bytes: usize,
}

trait ReceiptSink: MaybeSend + MaybeSync {
    fn acknowledge(&self, cursors: TopicCursor);
}

impl<F: Fn(TopicCursor) + MaybeSend + MaybeSync> ReceiptSink for F {
    fn acknowledge(&self, cursors: TopicCursor) {
        self(cursors);
    }
}

/// The receipt callback accepts only positions committed to local storage.
pub struct IncomingSubscription<E> {
    /// Ordered transport events. Reading an event does not commit receipt.
    pub events: BoxDynStream<'static, Result<IncomingEvent, E>>,
    receipt: Box<dyn ReceiptSink>,
}

impl<E> IncomingSubscription<E> {
    /// Attach a receipt callback to an owned event stream.
    pub fn new(
        events: BoxDynStream<'static, Result<IncomingEvent, E>>,
        receipt: impl Fn(TopicCursor) + MaybeSend + MaybeSync + 'static,
    ) -> Self {
        Self {
            events,
            receipt: Box::new(receipt),
        }
    }

    /// Advance resume positions only to the committed received prefix `F`.
    /// Do not acknowledge positions that exist only in memory.
    pub fn acknowledge_received(&self, cursors: TopicCursor) {
        self.receipt.acknowledge(cursors);
    }

    /// Convert transport errors without changing the receipt callback.
    pub fn map_error<U, F>(self, map: F) -> IncomingSubscription<U>
    where
        E: 'static,
        U: 'static,
        F: Fn(E) -> U + MaybeSend + 'static,
    {
        IncomingSubscription {
            events: Box::pin(self.events.map(move |event| event.map_err(&map))),
            receipt: self.receipt,
        }
    }
}

/// One ordered delivery from a topic. `after` is the cursor used for this read.
#[derive(Clone, Debug)]
pub struct OrderedEnvelopeBatch {
    /// The single topic shared by every envelope in this batch.
    pub topic: Topic,
    /// Read position before this batch, not a durable receipt acknowledgement.
    pub after: Cursor,
    /// Envelopes in strictly increasing sequence order. Sequence gaps are valid.
    pub envelopes: Vec<ServerEnvelope>,
}

/// Transport events do not mean that an envelope is stored or processed.
#[derive(Clone, Debug)]
pub enum IncomingEvent {
    /// Fixed catch-up targets for the topics in one accepted registration.
    Registered {
        /// Read positions used when these topics were registered.
        starts: TopicCursor,
        /// Targets captured at registration. Later messages do not raise them.
        targets: TopicCursor,
    },
    /// A validated transport batch whose raw bytes still need durable receipt.
    OrderedBatch(OrderedEnvelopeBatch),
    /// The feed ended. Reopen from committed receipt positions.
    Disconnected,
}
