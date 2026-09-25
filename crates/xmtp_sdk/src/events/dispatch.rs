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

use super::{ClientEvent, EventListener, ListenerError, ListenerID};
use crate::{XmtpError, foreign};

type StartGate = Mutex<bool>;

struct ListenerControl {
    subscription: Arc<Subscription<InternalEvent>>,
    stopped: watch::Sender<bool>,
    start_gate: Arc<StartGate>,
}

#[derive(Default)]
struct RegistryState {
    closing: bool,
    listeners: HashMap<u64, Arc<ListenerControl>>,
}

#[cfg(test)]
pub(crate) struct StartHook {
    pub(crate) arrived: tokio::sync::Notify,
    release: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

#[cfg(test)]
impl StartHook {
    pub(crate) fn new() -> (Arc<Self>, std::sync::mpsc::Sender<()>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        (
            Arc::new(Self {
                arrived: tokio::sync::Notify::new(),
                release: Mutex::new(Some(receiver)),
            }),
            sender,
        )
    }

    fn block_once(&self) {
        if let Some(receiver) = self.release.lock().take() {
            self.arrived.notify_one();
            let _ = receiver.recv();
        }
    }
}

#[derive(Default)]
pub(crate) struct ListenerRegistry {
    next_id: AtomicU64,
    state: Mutex<RegistryState>,
    #[cfg(test)]
    start_hook: Mutex<Option<Arc<StartHook>>>,
    #[cfg(test)]
    registration_hook: Mutex<Option<Arc<StartHook>>>,
    #[cfg(test)]
    stop_hook: Mutex<Option<Arc<StartHook>>>,
}

impl ListenerRegistry {
    #[cfg(test)]
    pub(crate) fn set_start_hook_for_test(&self, hook: Arc<StartHook>) {
        *self.start_hook.lock() = Some(hook);
    }

    #[cfg(test)]
    pub(crate) fn set_registration_hook_for_test(&self, hook: Arc<StartHook>) {
        *self.registration_hook.lock() = Some(hook);
    }

    #[cfg(test)]
    pub(crate) fn set_stop_hook_for_test(&self, hook: Arc<StartHook>) {
        *self.stop_hook.lock() = Some(hook);
    }

    #[cfg(test)]
    pub(crate) fn active_count_for_test(&self) -> usize {
        self.state.lock().listeners.len()
    }

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
        let start_gate = Arc::new(StartGate::new(false));
        #[cfg(test)]
        if let Some(hook) = self.registration_hook.lock().take() {
            hook.block_once();
        }
        let registered = {
            let mut state = self.state.lock();
            if state.closing {
                false
            } else {
                state.listeners.insert(
                    id,
                    Arc::new(ListenerControl {
                        subscription: subscription.clone(),
                        stopped,
                        start_gate: start_gate.clone(),
                    }),
                );
                true
            }
        };
        if !registered {
            subscription.close();
            return Err(XmtpError::closed());
        }
        spawn_dispatch(
            subscription,
            listener,
            receiver,
            start_gate,
            #[cfg(test)]
            self.start_hook.lock().clone(),
        );
        Ok(ListenerID(id))
    }

    pub(crate) fn stop(&self, id: ListenerID) {
        let control = {
            let mut state = self.state.lock();
            let control = state.listeners.remove(&id.0);
            if let Some(control) = &control {
                *control.start_gate.lock() = true;
                let _ = control.stopped.send(true);
            }
            control
        };
        #[cfg(test)]
        if let Some(hook) = self.stop_hook.lock().take() {
            hook.block_once();
        }
        if let Some(control) = control {
            control.subscription.close();
        }
    }

    pub(crate) fn stop_all(&self) {
        let listeners = {
            let mut state = self.state.lock();
            state.closing = true;
            std::mem::take(&mut state.listeners)
        };
        for control in listeners.into_values() {
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

/// Check for stop before the foreign call. The host checks again before the app callback.
async fn call_listener(
    listener: Arc<dyn EventListener>,
    event: ClientEvent,
    start_gate: Arc<StartGate>,
    #[cfg(test)] start_hook: Option<Arc<StartHook>>,
) -> Result<(), ListenerError> {
    #[cfg(test)]
    if let Some(hook) = &start_hook {
        hook.block_once();
    }
    if *start_gate.lock() {
        return Ok(());
    }
    listener.on_event(event).await
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn_dispatch(
    subscription: Arc<Subscription<InternalEvent>>,
    listener: Arc<dyn EventListener>,
    mut stopped: watch::Receiver<bool>,
    start_gate: Arc<StartGate>,
    #[cfg(test)] start_hook: Option<Arc<StartHook>>,
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
            #[cfg(test)]
            let start_hook = start_hook.clone();
            let call = tokio::spawn(async move {
                foreign::call(call_listener(
                    listener,
                    event,
                    start_gate,
                    #[cfg(test)]
                    start_hook,
                ))
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
    start_gate: Arc<StartGate>,
    #[cfg(test)] start_hook: Option<Arc<StartHook>>,
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
            #[cfg(test)]
            let start_hook = start_hook.clone();
            let (sender, receiver) = futures::channel::oneshot::channel();
            wasm_bindgen_futures::spawn_local(async move {
                let _ = sender.send(
                    foreign::call(call_listener(
                        listener,
                        event,
                        start_gate,
                        #[cfg(test)]
                        start_hook,
                    ))
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
