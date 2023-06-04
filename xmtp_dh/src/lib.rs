use xmtp_crypto::k256_helper;

// Uniffi requires enum errors that implement std::Error. We implement it
// manually here rather than pulling in thiserror to save binary size and compilation time.
#[derive(Debug)]
pub enum DiffieHellmanError {
    GenericError(String),
}

impl std::error::Error for DiffieHellmanError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match *self {
            DiffieHellmanError::GenericError(_) => None,
        }
    }
}

impl std::fmt::Display for DiffieHellmanError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match *self {
            DiffieHellmanError::GenericError(ref message) => write!(f, "{}", message),
        }
    }
}

pub fn diffie_hellman_k256(
    private_key_bytes: Vec<u8>,
    public_key_bytes: Vec<u8>,
) -> Result<Vec<u8>, DiffieHellmanError> {
    let shared_secret = k256_helper::diffie_hellman_byte_params(
        private_key_bytes.as_slice(),
        public_key_bytes.as_slice(),
    )
    .map_err(DiffieHellmanError::GenericError)?;
    Ok(shared_secret)
}

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard},
    task::{Context, Poll, Waker},
    thread,
    time::Duration,
};

/// Non-blocking timer future.
pub struct TimerFuture {
    shared_state: Arc<Mutex<SharedState>>,
}

struct SharedState {
    completed: bool,
    waker: Option<Waker>,
}

impl Future for TimerFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut shared_state = self.shared_state.lock().unwrap();

        if shared_state.completed {
            Poll::Ready(())
        } else {
            shared_state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl TimerFuture {
    pub fn new(duration: Duration) -> Self {
        let shared_state = Arc::new(Mutex::new(SharedState {
            completed: false,
            waker: None,
        }));

        let thread_shared_state = shared_state.clone();

        // Let's mimic an event coming from somewhere else, like the system.
        thread::spawn(move || {
            thread::sleep(duration);

            let mut shared_state: MutexGuard<_> = thread_shared_state.lock().unwrap();
            shared_state.completed = true;

            if let Some(waker) = shared_state.waker.take() {
                waker.wake();
            }
        });

        Self { shared_state }
    }
}

/// Async function that sleeps!
#[uniffi::export]
pub async fn sleep(ms: u16) -> bool {
    TimerFuture::new(Duration::from_millis(ms.into())).await;

    true
}

uniffi_macros::include_scaffolding!("xmtp_dh");
