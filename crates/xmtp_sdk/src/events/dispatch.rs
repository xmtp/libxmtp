use parking_lot::Mutex;
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};
use tokio::sync::watch;
use xmtp_events::Subscription;
use xmtp_mls::subscriptions::internal::InternalEvent;

use super::{ClientEvent, EventListener, ListenerID};
use crate::{XmtpError, foreign};

struct ListenerControl {
    subscription: Arc<Subscription<InternalEvent>>,
    stopped: watch::Sender<bool>,
    start_gate: Arc<Mutex<bool>>,
}

#[derive(Default)]
pub(crate) struct ListenerRegistry {
    next_id: AtomicU64,
    listeners: Mutex<HashMap<u64, ListenerControl>>,
}

impl ListenerRegistry {
    pub(crate) fn start(
        &self,
        subscription: Subscription<InternalEvent>,
        listener: Arc<dyn EventListener>,
    ) -> Result<ListenerID, XmtpError> {
        let id = self
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .map_err(|_| XmtpError::unknown("listener ID space exhausted"))?;
        let subscription = Arc::new(subscription);
        let (stopped, receiver) = watch::channel(false);
        let start_gate = Arc::new(Mutex::new(false));
        self.listeners.lock().insert(
            id,
            ListenerControl {
                subscription: subscription.clone(),
                stopped,
                start_gate: start_gate.clone(),
            },
        );
        spawn_dispatch(subscription, listener, receiver, start_gate);
        Ok(ListenerID(id))
    }

    pub(crate) fn stop(&self, id: ListenerID) {
        if let Some(control) = self.listeners.lock().remove(&id.0) {
            *control.start_gate.lock() = true;
            let _ = control.stopped.send(true);
            control.subscription.close();
        }
    }

    pub(crate) fn stop_all(&self) {
        for (_, control) in self.listeners.lock().drain() {
            *control.start_gate.lock() = true;
            let _ = control.stopped.send(true);
            control.subscription.close();
        }
    }
}

impl Drop for ListenerRegistry {
    fn drop(&mut self) {
        self.stop_all();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_dispatch(
    subscription: Arc<Subscription<InternalEvent>>,
    listener: Arc<dyn EventListener>,
    mut stopped: watch::Receiver<bool>,
    start_gate: Arc<Mutex<bool>>,
) {
    tokio::spawn(async move {
        loop {
            let lease = tokio::select! {
                value = subscription.next_for_callback() => value,
                _ = stopped.changed() => break,
            };
            let Some(lease) = lease else { break };
            let Some(event) = lease.event.client.clone().map(ClientEvent::from) else {
                continue;
            };
            if *stopped.borrow() || subscription.is_closed() {
                break;
            }
            let listener = listener.clone();
            let start_gate = start_gate.clone();
            let call = tokio::spawn(async move {
                foreign::call(async move {
                    let call = {
                        let stopped = start_gate.lock();
                        if *stopped {
                            return Ok(());
                        }
                        listener.on_event(event)
                    };
                    call.await
                })
                .await
            });
            tokio::select! {
                result = call => {
                    if !matches!(result, Ok(Ok(Ok(())))) {
                        tracing::warn!("event listener callback failed");
                    }
                }
                _ = stopped.changed() => break,
            }
            drop(lease);
        }
    });
}

#[cfg(target_arch = "wasm32")]
fn spawn_dispatch(
    subscription: Arc<Subscription<InternalEvent>>,
    listener: Arc<dyn EventListener>,
    mut stopped: watch::Receiver<bool>,
    start_gate: Arc<Mutex<bool>>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        loop {
            let lease = futures::select_biased! {
                value = subscription.next_for_callback().fuse() => value,
                _ = stopped.changed().fuse() => break,
            };
            let Some(lease) = lease else { break };
            let Some(event) = lease.event.client.clone().map(ClientEvent::from) else {
                continue;
            };
            if *stopped.borrow() || subscription.is_closed() {
                break;
            }
            let listener = listener.clone();
            let start_gate = start_gate.clone();
            let (sender, receiver) = futures::channel::oneshot::channel();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = sender.send(
                    foreign::call(async move {
                        let call = {
                            let stopped = start_gate.lock();
                            if *stopped {
                                return Ok(());
                            }
                            listener.on_event(event)
                        };
                        call.await
                    })
                    .await,
                );
            });
            use futures::FutureExt;
            futures::select_biased! {
                result = receiver.fuse() => {
                    if !matches!(result, Ok(Ok(Ok(())))) { tracing::warn!("event listener callback failed"); }
                }
                _ = stopped.changed().fuse() => break,
            }
            drop(lease);
        }
    });
}
