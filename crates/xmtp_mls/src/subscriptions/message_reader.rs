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
    configuration: crate::server_configuration::ServerConfigurationHandle,
}

/// Updates delivery selection and observes network progress without consuming messages.
#[derive(Clone)]
pub struct MessageReaderControl {
    delivery: LocalDeliveryControl,
    lease: Arc<Mutex<Option<Arc<IncomingLease>>>>,
    changes: Arc<tokio::sync::Mutex<tokio::sync::watch::Receiver<u64>>>,
    last_status: Arc<Mutex<IncomingStatus>>,
    closed: tokio_util::sync::CancellationToken,
}

/// One independent status observer for a caller that snapshots before waiting.
pub struct MessageReaderObserver {
    changes: Option<tokio::sync::watch::Receiver<u64>>,
    closed: tokio_util::sync::CancellationToken,
}

impl MessageReaderObserver {
    pub async fn changed(&mut self) {
        if let Some(changes) = &mut self.changes {
            tokio::select! {
                _ = self.closed.cancelled() => {},
                _ = changes.changed() => {},
            }
        }
    }
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
        context
            .server_configuration()
            .check()
            .map_err(|error| LocalDeliveryError::Configuration(Box::new(error)))?;
        let configuration = context.server_configuration().clone();
        let config = LocalDeliveryConfig::from(context.incoming_runtime().policy());
        let coordinator = IncomingCoordinator::for_context(&context);
        let lease = Arc::new(coordinator.acquire_stream(incoming_scope(&scope)));
        let delivery = LocalDelivery::new(context, scope, filter, from, config)?;
        let control = MessageReaderControl {
            delivery: delivery.control(),
            last_status: Arc::new(Mutex::new(lease.snapshot())),
            changes: Arc::new(tokio::sync::Mutex::new(lease.subscribe_changes())),
            lease: Arc::new(Mutex::new(Some(lease))),
            closed: tokio_util::sync::CancellationToken::new(),
        };
        Ok(Self {
            delivery,
            control,
            configuration,
        })
    }

    pub fn control(&self) -> MessageReaderControl {
        self.control.clone()
    }

    /// Return one unacknowledged item; receipt can continue while its host handles it.
    pub async fn next_delivery(
        &mut self,
    ) -> Result<Option<LocalDeliveryItem<C>>, LocalDeliveryError> {
        let lease = self.control.lease.lock().clone();
        let Some(lease) = lease else { return Ok(None) };
        let configuration = self.configuration.clone();
        let result = {
            // Keep the pending read alive across status hints. Cancelling it on
            // each hint could starve local storage work during a busy stream.
            let delivery = self.delivery.next_delivery();
            futures::pin_mut!(delivery);
            loop {
                if let Err(error) = configuration.check() {
                    break Err(LocalDeliveryError::Configuration(Box::new(error)));
                }
                if let Err(error) = lease.check_recovery() {
                    break Err(error.error());
                }
                tokio::select! {
                    biased;
                    _ = self.control.closed.cancelled() => {
                        break configuration.check()
                            .map(|()| None)
                            .map_err(|error| LocalDeliveryError::Configuration(Box::new(error)));
                    },
                    item = &mut delivery => {
                        if let Err(error) = configuration.check() {
                            break Err(LocalDeliveryError::Configuration(Box::new(error)));
                        }
                        // Both futures can become ready while this task is suspended.
                        // An ended lease must win before another item reaches the app.
                        if matches!(&item, Ok(Some(_)))
                            && let Err(error) = lease.check_recovery()
                        {
                            break Err(error.error());
                        }
                        break item;
                    },
                    _ = lease.changed() => {},
                    _ = xmtp_common::time::sleep(super::recovery::RECOVERY_POLL) => {},
                }
            }
        };
        if !matches!(&result, Ok(Some(_))) {
            // Release network interest and ownership on both error and EOF.
            // The caller can immediately open a replacement stream.
            self.close();
        }
        result
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
    /// Create a receiver before the caller reads its first status snapshot.
    pub fn observer(&self) -> MessageReaderObserver {
        MessageReaderObserver {
            changes: self
                .lease
                .lock()
                .as_ref()
                .map(|lease| lease.subscribe_changes()),
            closed: self.closed.clone(),
        }
    }

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
        let mut changes = self.changes.lock().await;
        tokio::select! {
            _ = self.closed.cancelled() => {},
            _ = changes.changed() => {},
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test::mock::generate_stored_msg, tester};
    use xmtp_common::time::{Duration, Instant};
    use xmtp_db::Store;
    use xmtp_proto::types::Cursor;

    // verifies: CONF-075
    #[xmtp_common::test(unwrap_try = true)]
    async fn blocked_connection_ends_a_pending_message_read_with_its_cause() {
        use crate::{client::ClientError, server_configuration::BlockedConnection};
        tester!(alix, disable_workers);
        let mut reader = MessageReader::new(
            alix.context.clone(),
            DeliveryScope::All,
            LocalDeliveryFilter::default(),
            None,
        )?;
        let error = {
            let pending = reader.next_delivery();
            futures::pin_mut!(pending);
            assert!(futures::poll!(&mut pending).is_pending());
            alix.context.server_configuration().block_connection(
                BlockedConnection::ClientVersionTooOld {
                    client: "1.0.0".into(),
                    minimum: "9999.0.0".into(),
                },
            );
            alix.context.cancellation_token().cancel();
            pending
                .await
                .err()
                .expect("blocked connection must fail the read")
        };
        assert!(matches!(
            error,
            LocalDeliveryError::Configuration(error)
                if matches!(*error, ClientError::ClientVersionTooOld { ref minimum, .. }
                    if minimum == "9999.0.0")
        ));
        assert!(reader.next_delivery().await?.is_none());
    }

    // verifies: PROC-038
    #[xmtp_common::test(unwrap_try = true)]
    async fn exhaustion_wins_when_a_pending_local_item_is_ready_too() {
        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        for sequence in [100, 200] {
            generate_stored_msg(Cursor(sequence), group.group_id).store(&alix.context.db())?;
        }
        let create = || {
            MessageReader::new(
                alix.context.clone(),
                DeliveryScope::Groups(vec![group.group_id]),
                LocalDeliveryFilter::default(),
                None,
            )
        };
        let mut reader = create()?;
        let lease = reader.control.lease.lock().clone().unwrap();
        let first = reader.next_delivery().await?.unwrap();
        first.acknowledgement.check_owner()?;
        let pending = reader.next_delivery();
        futures::pin_mut!(pending);
        assert!(futures::poll!(&mut pending).is_pending());
        lease.fail_recovery_for_test(super::super::recovery::RecoveryFailure::Exhausted {
            attempts: 10,
            source: None,
        });
        // An already handed-off callback may finish. Its next local item is ready
        // at the same time as the retained network failure.
        first.acknowledgement.acknowledge()?;
        assert!(matches!(
            pending.await,
            Err(LocalDeliveryError::NetworkRecoveryExhausted { .. })
        ));
        let mut replacement = create()?;
        let second = replacement.next_delivery().await?.unwrap();
        assert_ne!(second.message.id, first.message.id);
    }

    // verifies: PROC-039
    #[xmtp_common::test(unwrap_try = true)]
    async fn twice_exhausted_readers_release_ownership_before_replacement_on_same_client() {
        tester!(alix, disable_workers);
        let group = alix.create_group(None, None)?;
        let create = || {
            MessageReader::new(
                alix.context.clone(),
                DeliveryScope::Groups(vec![group.group_id]),
                LocalDeliveryFilter::default(),
                None,
            )
        };
        for sequence in [100, 200] {
            let message = generate_stored_msg(Cursor(sequence), group.group_id);
            message.store(&alix.context.db())?;
            let mut expired = create()?;
            let lease = expired.control.lease.lock().clone().unwrap();
            // Observe a future outage deadline without subtracting from the
            // WASM clock. Publish the failure only to this consumer's lease.
            let outage = super::super::recovery::RecoverySnapshot::default();
            let now = Instant::now();
            let mut recovery = super::super::recovery::RecoveryBudget::new(&outage, now);
            let failure = recovery
                .check(&outage, now + Duration::from_secs(3600))
                .unwrap_err();
            assert!(matches!(
                failure,
                super::super::recovery::RecoveryFailure::Exhausted { .. }
            ));
            lease.fail_recovery_for_test(failure);
            assert!(matches!(
                expired.next_delivery().await,
                Err(LocalDeliveryError::NetworkRecoveryExhausted { .. })
            ));
            let mut replacement = create()?;
            expired.close();
            let delivered = replacement.next_delivery().await?.unwrap();
            assert_eq!(delivered.message.id, message.id);
            delivered.acknowledgement.check_owner()?;
            delivered.acknowledgement.acknowledge()?;
            replacement.close();
        }
    }
}
