//! Conformance-only control of the real native constructor's adoption point.
use super::*;
use xmtp_db::ConnectionExt;

#[cfg(not(test))]
const COMPLETION_WAIT_LIMIT: xmtp_common::time::Duration =
    xmtp_common::time::Duration::from_secs(30);
#[cfg(test)]
const COMPLETION_WAIT_LIMIT: xmtp_common::time::Duration =
    xmtp_common::time::Duration::from_millis(20);

struct ShutdownGate {
    entered: std::sync::atomic::AtomicBool,
    released: std::sync::atomic::AtomicBool,
    release: tokio::sync::Notify,
}

impl ShutdownGate {
    fn release(&self) {
        self.released.store(true, Ordering::SeqCst);
        self.release.notify_waiters();
    }
}

static SHUTDOWN_GATES: std::sync::LazyLock<
    parking_lot::Mutex<std::collections::HashMap<usize, std::sync::Weak<ShutdownGate>>>,
> = std::sync::LazyLock::new(Default::default);

pub(super) async fn pause_shutdown(client: &CoreClient) {
    let gate = SHUTDOWN_GATES
        .lock()
        .get(&(client as *const CoreClient as usize))
        .and_then(std::sync::Weak::upgrade);
    let Some(gate) = gate else { return };
    gate.entered.store(true, Ordering::SeqCst);
    loop {
        let released = gate.release.notified();
        tokio::pin!(released);
        released.as_mut().enable();
        if gate.released.load(Ordering::SeqCst) {
            return;
        }
        released.await;
    }
}
use xmtp_db::diesel::{RunQueryDsl, sql_query};

#[derive(uniffi::Record)]
pub struct SdkConformanceConstructorState {
    pub store_open_reported: bool,
    pub client_captured: bool,
    pub client_closed: bool,
    pub workers_stopped: bool,
    pub store_connected: bool,
}

/// Use one probe at a time. The store diagnostic is process-wide.
#[derive(uniffi::Object)]
pub struct SdkConformanceConstructorProbe {
    probe: Arc<build_task_probe::BuildTaskProbe>,
    adopted: parking_lot::Mutex<Option<Client>>,
    ready_client: parking_lot::Mutex<Option<std::sync::Weak<Client>>>,
    shutdown: parking_lot::Mutex<Option<Arc<ShutdownGate>>>,
    #[cfg(test)]
    cleanup_end_failure: std::sync::atomic::AtomicBool,
}

#[xmtp_macro::sdk_export(native_only)]
impl SdkConformanceConstructorProbe {
    #[uniffi::constructor]
    pub async fn open() -> Self {
        STORE_LEFT_OPEN.store(false, Ordering::SeqCst);
        let probe = Arc::new(build_task_probe::BuildTaskProbe::default());
        probe.hold_adoption.store(true, Ordering::SeqCst);
        Self {
            probe,
            adopted: parking_lot::Mutex::new(None),
            ready_client: parking_lot::Mutex::new(None),
            shutdown: parking_lot::Mutex::new(None),
            #[cfg(test)]
            cleanup_end_failure: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub async fn create(
        &self,
        signer: Arc<dyn Signer>,
        options: ClientOptions,
    ) -> Result<(), XmtpError> {
        let client = build_task_probe::CURRENT
            .scope(self.probe.clone(), Client::create(signer, options))
            .await?;
        *self.adopted.lock() = Some(client);
        Ok(())
    }

    pub async fn build(
        &self,
        identity: PublicIdentity,
        options: ClientOptions,
        inbox_id: Option<InboxId>,
    ) -> Result<(), XmtpError> {
        let client = build_task_probe::CURRENT
            .scope(
                self.probe.clone(),
                Client::build(identity, options, inbox_id),
            )
            .await?;
        *self.adopted.lock() = Some(client);
        Ok(())
    }

    /// Return a real client and observe its FFI owner without retaining it.
    pub async fn create_ready(
        &self,
        signer: Arc<dyn Signer>,
        options: ClientOptions,
    ) -> Result<Arc<Client>, XmtpError> {
        self.probe.hold_adoption.store(false, Ordering::SeqCst);
        let client = Arc::new(
            build_task_probe::CURRENT
                .scope(self.probe.clone(), Client::create(signer, options))
                .await?,
        );
        *self.ready_client.lock() = Some(Arc::downgrade(&client));
        Ok(client)
    }

    /// Return a built client through the same ready-result ownership boundary.
    pub async fn build_ready(
        &self,
        identity: PublicIdentity,
        options: ClientOptions,
        inbox_id: Option<InboxId>,
    ) -> Result<Arc<Client>, XmtpError> {
        self.probe.hold_adoption.store(false, Ordering::SeqCst);
        let client = Arc::new(
            build_task_probe::CURRENT
                .scope(
                    self.probe.clone(),
                    Client::build(identity, options, inbox_id),
                )
                .await?,
        );
        *self.ready_client.lock() = Some(Arc::downgrade(&client));
        Ok(client)
    }

    pub fn ready_client_alive(&self) -> bool {
        self.ready_client
            .lock()
            .as_ref()
            .is_some_and(|client| client.strong_count() != 0)
    }

    /// Observe a real client for private shutdown controls. @xmtp-worker @xmtp-internal
    pub fn observe_client(&self, client: Arc<Client>) {
        self.release_shutdown();
        *self.probe.client.lock() = Some(client.inner.clone());
    }

    /// Hold native shutdown after its reader-close step. @xmtp-worker @xmtp-internal
    pub fn hold_shutdown(&self) {
        self.release_shutdown();
        let client = self.probe.client.lock().clone().expect("captured client");
        let gate = Arc::new(ShutdownGate {
            entered: std::sync::atomic::AtomicBool::new(false),
            released: std::sync::atomic::AtomicBool::new(false),
            release: tokio::sync::Notify::new(),
        });
        let mut gates = SHUTDOWN_GATES.lock();
        gates.retain(|_, gate| gate.strong_count() != 0);
        gates.insert(Arc::as_ptr(&client) as usize, Arc::downgrade(&gate));
        *self.shutdown.lock() = Some(gate);
    }

    /// Read the private native shutdown boundary. @xmtp-worker @xmtp-internal
    pub fn shutdown_entered(&self) -> bool {
        self.shutdown
            .lock()
            .as_ref()
            .is_some_and(|gate| gate.entered.load(Ordering::SeqCst))
    }

    /// Release the held native shutdown. @xmtp-worker @xmtp-internal
    pub fn release_shutdown(&self) {
        if let Some(gate) = self.shutdown.lock().take() {
            gate.release();
        }
    }

    /// Wait for task completion while the parent still owns the unconsumed output.
    pub async fn wait_for_completed(&self) -> Result<(), XmtpError> {
        xmtp_common::time::timeout(COMPLETION_WAIT_LIMIT, async {
            self.probe.started.notified().await;
            let task = self.probe.task.lock().clone().expect("constructor task");
            while !task.is_finished() {
                xmtp_common::time::sleep(xmtp_common::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .map_err(|_| XmtpError::unknown("constructor did not complete at the adoption barrier"))
    }

    pub fn release(&self) {
        self.probe.release_adoption.notify_one();
    }

    pub fn state(&self) -> SdkConformanceConstructorState {
        let client = self.probe.client.lock();
        SdkConformanceConstructorState {
            store_open_reported: STORE_LEFT_OPEN.load(Ordering::SeqCst),
            client_captured: client.is_some(),
            client_closed: client
                .as_ref()
                .is_some_and(|client| client.context.is_closed()),
            workers_stopped: client
                .as_ref()
                .is_some_and(|client| client.context.shutdown_complete()),
            store_connected: client.as_ref().is_some_and(|client| {
                client
                    .context
                    .db()
                    .raw_query(|conn| sql_query("SELECT 1").execute(conn))
                    .is_ok()
            }),
        }
    }

    /// Wait for the cleanup that cancellation schedules, without initiating it.
    pub async fn wait_for_cleanup(&self) -> Result<(), XmtpError> {
        xmtp_common::time::timeout(xmtp_common::time::Duration::from_secs(10), async {
            loop {
                if self.state().workers_stopped {
                    return;
                }
                xmtp_common::time::sleep(xmtp_common::time::Duration::from_millis(1)).await;
            }
        })
        .await
        .map_err(|_| XmtpError::unknown("unadopted constructor did not stop its workers"))
    }

    /// End the client that the completed parent adopted.
    pub async fn end_adopted(&self) -> Result<(), XmtpError> {
        #[cfg(test)]
        if self.cleanup_end_failure.swap(false, Ordering::SeqCst) {
            return Err(XmtpError::unknown("constructor cleanup end failed"));
        }
        let client = self.adopted.lock().take();
        if let Some(client) = client {
            client.end().await?;
        }
        Ok(())
    }

    /// Always call this after a probe failure so the test cannot leave workers.
    pub async fn cleanup(&self) -> Result<(), XmtpError> {
        self.release_shutdown();
        self.release();
        let ended = self.end_adopted().await;
        let client = self.probe.client.lock().clone();
        let closed = if let Some(client) = client {
            client.close().await.map_err(XmtpError::from_client)
        } else {
            Ok(())
        };
        ended.and(closed)
    }
}

impl Drop for SdkConformanceConstructorProbe {
    fn drop(&mut self) {
        if let Some(gate) = self.shutdown.get_mut().take() {
            gate.release();
        }
    }
}

#[cfg(test)]
mod tests;
