//! Native callback lifetime probes for each foreign callback family.
use super::*;
use std::{
    collections::HashSet,
    pin::Pin,
    sync::atomic::AtomicUsize,
    task::{Context, Poll},
    thread::ThreadId,
};
use tokio::sync::{mpsc, oneshot};

const CALLS: usize = 32;
const CYCLES: usize = 20;
const DEADLINE: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Counts {
    active: AtomicUsize,
    retained: AtomicUsize,
    early_drops: AtomicUsize,
    polls: parking_lot::Mutex<Vec<ThreadId>>,
}

struct HeldFuture {
    release: oneshot::Receiver<()>,
    entered: Option<mpsc::UnboundedSender<()>>,
    counts: Arc<Counts>,
    complete: bool,
    started: bool,
}

impl Future for HeldFuture {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.counts.polls.lock().push(std::thread::current().id());
        if !self.started {
            self.started = true;
            self.counts.active.fetch_add(1, Ordering::SeqCst);
            self.entered.take().unwrap().send(()).unwrap();
        }
        match Pin::new(&mut self.release).poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                result.expect("the probe must release each callback");
                self.complete = true;
                Poll::Ready(())
            }
        }
    }
}

impl Drop for HeldFuture {
    fn drop(&mut self) {
        if self.started {
            self.counts.active.fetch_sub(1, Ordering::SeqCst);
            if !self.complete {
                self.counts.early_drops.fetch_add(1, Ordering::SeqCst);
            }
        }
    }
}

struct HeldCallback {
    future: parking_lot::Mutex<Option<HeldFuture>>,
    counts: Arc<Counts>,
    dropped: mpsc::UnboundedSender<()>,
}

impl HeldCallback {
    async fn hold(&self) {
        let future = self.future.lock().take().expect("one call per callback");
        future.await;
    }
}

impl Drop for HeldCallback {
    fn drop(&mut self) {
        self.counts.retained.fetch_sub(1, Ordering::SeqCst);
        let _ = self.dropped.send(());
    }
}

#[xmtp_common::async_trait]
impl Signer for HeldCallback {
    async fn identity(&self) -> Result<PublicIdentity, SignerError> {
        self.hold().await;
        Ok(PublicIdentity {
            identifier: "0x1111111111111111111111111111111111111111".into(),
            kind: PublicIdentityKind::Ethereum,
        })
    }
    async fn kind(&self) -> Result<SignerKind, SignerError> {
        self.hold().await;
        Ok(SignerKind::Eoa)
    }
    async fn sign(&self, _: SigningRequest) -> Result<Signature, SignerError> {
        self.hold().await;
        Ok(Signature::Ecdsa(vec![]))
    }
}

#[xmtp_common::async_trait]
impl CredentialSource for HeldCallback {
    async fn credential(&self) -> Result<Credential, CredentialError> {
        self.hold().await;
        Ok(Credential {
            name: None,
            value: "probe".into(),
            expires_at_seconds: i64::MAX,
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum Family {
    Identity,
    Kind,
    Sign,
    Credential,
    Helper,
}

async fn invoke(family: Family, callback: Arc<HeldCallback>) {
    match family {
        Family::Identity => {
            signer::identity(callback).await.unwrap();
        }
        Family::Kind => {
            signer::kind(callback).await.unwrap();
        }
        Family::Sign => {
            signer::sign(
                callback,
                SigningRequest {
                    text: "probe".into(),
                },
            )
            .await
            .unwrap();
        }
        Family::Credential => {
            xmtp_api_backend::AuthCallback::on_auth_required(&AuthBridge::new(callback))
                .await
                .unwrap();
        }
        Family::Helper => {
            crate::foreign::call(async move { callback.hold().await })
                .await
                .unwrap();
        }
    }
}

async fn cycle(family: Family, cancel: bool) {
    let executor = std::thread::current().id();
    let counts = Arc::new(Counts::default());
    let (entered, mut entries) = mpsc::unbounded_channel();
    let (dropped, mut drops) = mpsc::unbounded_channel();
    let mut releases = Vec::new();
    let mut calls = Vec::new();
    for _ in 0..CALLS {
        let (release, receiver) = oneshot::channel();
        releases.push(release);
        counts.retained.fetch_add(1, Ordering::SeqCst);
        let callback = Arc::new(HeldCallback {
            future: parking_lot::Mutex::new(Some(HeldFuture {
                release: receiver,
                entered: Some(entered.clone()),
                counts: counts.clone(),
                complete: false,
                started: false,
            })),
            counts: counts.clone(),
            dropped: dropped.clone(),
        });
        calls.push(invoke(family, callback));
    }
    let mut callers = Box::pin(futures::future::join_all(calls));
    let all_entered = async {
        for _ in 0..CALLS {
            entries.recv().await.unwrap();
        }
    };
    let admitted = xmtp_common::time::timeout(DEADLINE, async {
        tokio::select! {
            _ = &mut callers => panic!("callbacks returned before release"),
            _ = all_entered => {}
        }
    })
    .await;
    // Release even when entry fails. A failed probe must not hang the runtime.
    if admitted.is_err() {
        for release in releases {
            let _ = release.send(());
        }
        panic!("{family:?}: 32 callbacks did not enter");
    }
    assert_eq!(counts.active.load(Ordering::SeqCst), CALLS);
    assert_eq!(counts.retained.load(Ordering::SeqCst), CALLS);
    let held_polls = counts.polls.lock().len();
    let mut callers = Some(callers);
    if cancel {
        drop(callers.take());
    }
    for release in releases {
        let _ = release.send(());
    }
    if let Some(callers) = callers {
        xmtp_common::time::timeout(DEADLINE, callers).await.unwrap();
    }
    xmtp_common::time::timeout(DEADLINE, async {
        for _ in 0..CALLS {
            drops.recv().await.unwrap();
        }
    })
    .await
    .expect("foreign callbacks stayed retained after release");
    assert_eq!(counts.active.load(Ordering::SeqCst), 0);
    assert_eq!(counts.retained.load(Ordering::SeqCst), 0);
    assert_eq!(counts.early_drops.load(Ordering::SeqCst), 0);
    let polls = counts.polls.lock();
    assert!(
        polls.len() >= held_polls + CALLS,
        "callbacks were not polled after release"
    );
    assert!(
        polls.iter().all(|thread| *thread != executor),
        "foreign future polled on the caller executor"
    );
    let threads: HashSet<_> = polls.iter().collect();
    assert!(
        threads.len() >= CALLS,
        "held foreign calls did not enter independently"
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn callback_lifetime_native_matrix() {
    for family in [
        Family::Identity,
        Family::Kind,
        Family::Sign,
        Family::Credential,
        Family::Helper,
    ] {
        for _ in 0..CYCLES {
            cycle(family, true).await;
            cycle(family, false).await;
        }
    }
}
