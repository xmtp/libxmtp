//! Message delivery with shared network receipt and processing status.

use parking_lot::Mutex;
use std::sync::Arc;

use super::{
    incoming::{IncomingCoordinator, IncomingLease, IncomingScope, IncomingStatus},
    local_delivery::{
        DeliveryCursor, DeliveryScope, LocalDelivery, LocalDeliveryConfig, LocalDeliveryControl,
        LocalDeliveryError, LocalDeliveryFilter, LocalDeliveryItem,
    },
};
use crate::context::XmtpSharedContext;

/// The network lease does not depend on application acknowledgement.
pub struct MessageReader<C: XmtpSharedContext> {
    delivery: LocalDelivery<C>,
    control: MessageReaderControl,
}

/// Updates delivery selection and observes network progress without consuming messages.
#[derive(Clone)]
pub struct MessageReaderControl {
    delivery: LocalDeliveryControl,
    lease: Arc<Mutex<Option<Arc<IncomingLease>>>>,
    last_status: Arc<Mutex<IncomingStatus>>,
    closed: tokio_util::sync::CancellationToken,
}

fn incoming_scope(scope: &DeliveryScope) -> IncomingScope {
    match scope {
        DeliveryScope::All => IncomingScope::AllGroups,
        DeliveryScope::Groups(groups) => IncomingScope::Groups(groups.clone()),
    }
}

impl<C: XmtpSharedContext + 'static> MessageReader<C> {
    /// Hold network interest while opening default delivery or independent `from` replay.
    pub fn new(
        context: C,
        scope: DeliveryScope,
        filter: LocalDeliveryFilter,
        from: Option<DeliveryCursor>,
    ) -> Result<Self, LocalDeliveryError> {
        let config = LocalDeliveryConfig::from(context.stream_settings());
        let coordinator = IncomingCoordinator::for_context(&context);
        let lease = Arc::new(coordinator.acquire(incoming_scope(&scope)));
        let delivery = LocalDelivery::new(context, scope, filter, from, config)?;
        let control = MessageReaderControl {
            delivery: delivery.control(),
            last_status: Arc::new(Mutex::new(lease.snapshot())),
            lease: Arc::new(Mutex::new(Some(lease))),
            closed: tokio_util::sync::CancellationToken::new(),
        };
        Ok(Self { delivery, control })
    }

    pub fn control(&self) -> MessageReaderControl {
        self.control.clone()
    }

    /// Return one unacknowledged item; receipt can continue while its host handles it.
    pub async fn next_delivery(
        &mut self,
    ) -> Result<Option<LocalDeliveryItem<C>>, LocalDeliveryError> {
        self.delivery.next_delivery().await
    }

    /// Report processing through fixed network heads, not application delivery progress.
    pub fn catch_up_snapshot(&self) -> IncomingStatus {
        self.control.catch_up_snapshot()
    }

    /// A Rust iterator acknowledges only when its consumer requests the next item.
    pub fn into_stream(
        self,
    ) -> impl futures::Stream<Item = super::Result<xmtp_db::group_message::StoredGroupMessage>>
    {
        futures::stream::unfold(
            Some((
                self,
                None::<super::local_delivery::DeliveryAcknowledgement<C>>,
            )),
            |state| async move {
                let (mut reader, previous) = state?;
                if let Some(previous) = previous
                    && let Err(error) = previous.acknowledge()
                {
                    return Some((Err(error.into()), None));
                }
                loop {
                    match reader.next_delivery().await {
                        Ok(Some(item)) => match item.acknowledgement.check_owner() {
                            Ok(()) => {
                                return Some((
                                    Ok(item.message),
                                    Some((reader, Some(item.acknowledgement))),
                                ));
                            }
                            Err(LocalDeliveryError::SelectionChanged) => continue,
                            Err(error) => return Some((Err(error.into()), None)),
                        },
                        Ok(None) => return None,
                        Err(error) => return Some((Err(error.into()), None)),
                    }
                }
            },
        )
    }

    /// Release network interest and delivery ownership without acknowledging the last item.
    pub fn close(&mut self) {
        self.delivery.close();
        self.control.close();
    }
}

impl MessageReaderControl {
    /// Replace network interest and local selection; excluded groups keep their saved D.
    pub fn update_scope(&self, scope: DeliveryScope) {
        self.delivery.update_scope(scope.clone());
        if let Some(lease) = self.lease.lock().as_ref() {
            lease.replace_scope(incoming_scope(&scope));
        }
    }

    /// Apply filters to future selection without rewinding saved delivery progress.
    pub fn update_filter(&self, filter: LocalDeliveryFilter) {
        self.delivery.update_filter(filter);
    }

    /// Read current obligations, or the final cancelled snapshot after close.
    pub fn catch_up_snapshot(&self) -> IncomingStatus {
        if let Some(lease) = self.lease.lock().as_ref() {
            let status = lease.snapshot();
            *self.last_status.lock() = status.clone();
            status
        } else {
            self.last_status.lock().clone()
        }
    }

    /// Wait for a status hint or close; read a fresh snapshot after this returns.
    pub async fn changed(&self) {
        let lease = self.lease.lock().clone();
        if let Some(lease) = lease {
            tokio::select! {
                _ = self.closed.cancelled() => {},
                _ = lease.changed() => {},
            }
        }
    }

    /// Cancel the scope and fence delivery tokens without changing durable progress.
    pub fn close(&self) {
        self.delivery.close();
        self.closed.cancel();
        if let Some(lease) = self.lease.lock().take() {
            let mut status = lease.snapshot();
            status.cancel();
            *self.last_status.lock() = status;
            lease.close();
        }
    }
}

impl<C: XmtpSharedContext> Drop for MessageReader<C> {
    fn drop(&mut self) {
        self.control.close();
    }
}
