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
    recovery: super::recovery::RecoveryBudget,
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
        let config = LocalDeliveryConfig::from(context.incoming_runtime().policy());
        let coordinator = IncomingCoordinator::for_context(&context);
        let lease = Arc::new(coordinator.acquire_stream(incoming_scope(&scope)));
        let delivery = LocalDelivery::new(context, scope, filter, from, config)?;
        let recovery = super::recovery::RecoveryBudget::new(
            &lease.recovery_snapshot(),
            xmtp_common::time::Instant::now(),
        );
        let control = MessageReaderControl {
            delivery: delivery.control(),
            last_status: Arc::new(Mutex::new(lease.snapshot())),
            lease: Arc::new(Mutex::new(Some(lease))),
            closed: tokio_util::sync::CancellationToken::new(),
        };
        Ok(Self {
            delivery,
            control,
            recovery,
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
        let result = {
            // Keep the pending read alive across status hints. Cancelling it on
            // each hint could starve local storage work during a busy stream.
            let delivery = self.delivery.next_delivery();
            futures::pin_mut!(delivery);
            loop {
                if let Err(error) = self.recovery.check(
                    &lease.recovery_snapshot(),
                    xmtp_common::time::Instant::now(),
                ) {
                    break Err(error.error());
                }
                tokio::select! {
                    biased;
                    _ = self.control.closed.cancelled() => break Ok(None),
                    item = &mut delivery => break item,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test::mock::generate_stored_msg, tester};
    use xmtp_common::time::{Duration, Instant};
    use xmtp_db::Store;
    use xmtp_proto::types::Cursor;

    xmtp_common::if_native! {
    // verifies: PROC-040
    #[xmtp_common::test(unwrap_try = true)]
    async fn failed_token_ends_delivery_and_releases_network_interest() {
        use xmtp_db::ConnectionExt;
        tester!(alix, persistent_db, disable_workers);
        let group = alix.create_group(None, None)?;
        let message = generate_stored_msg(Cursor(100), group.group_id);
        message.store(&alix.context.db())?;
        let create = || MessageReader::new(alix.context.clone(),
            DeliveryScope::Groups(vec![group.group_id]), LocalDeliveryFilter::default(), None);
        let mut reader = create()?;
        let control = reader.control();
        let item = reader.next_delivery().await?.unwrap();
        alix.context.db().disconnect()?;
        assert!(item.acknowledgement.check_owner().is_err());
        assert!(reader.next_delivery().await?.is_none());
        assert!(control.lease.lock().is_none());
        assert!(control.closed.is_cancelled());
        alix.context.db().reconnect()?;
        let mut replacement = create()?;
        assert_eq!(replacement.next_delivery().await?.unwrap().message.id, message.id);
    }
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
            expired.recovery = super::super::recovery::RecoveryBudget::new(&outage, now);
            let failure = expired
                .recovery
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
