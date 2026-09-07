use super::OUTBOUND_FRAMES;
use crate::{api, config::OUTBOUND_QUEUE_BYTES};
use futures::{Stream, task::AtomicWaker};
use parking_lot::Mutex;
use prost::Message;
use std::{
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
use tokio::sync::{Notify, OwnedSemaphorePermit, Semaphore, mpsc};
use tonic::Status;

#[derive(Default)]
pub(super) struct Terminal {
    error: Mutex<Option<Status>>,
    waker: AtomicWaker,
    pub wake: Notify,
    closed: AtomicBool,
}

impl Terminal {
    pub fn fail(&self, error: Status) {
        self.error.lock().get_or_insert(error);
        self.waker.wake();
        self.wake.notify_one();
    }
    pub fn error(&self) -> Option<Status> {
        self.error.lock().clone()
    }
    pub fn closed(&self) -> bool {
        self.closed.load(Ordering::Acquire)
    }
}

pub(super) struct Budget {
    bytes: Arc<Semaphore>,
    frames: Arc<Semaphore>,
    pub wake: Arc<Notify>,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            bytes: Arc::new(Semaphore::new(OUTBOUND_QUEUE_BYTES)),
            frames: Arc::new(Semaphore::new(OUTBOUND_FRAMES)),
            wake: Arc::new(Notify::new()),
        }
    }
}

impl Budget {
    pub fn available(&self) -> usize {
        self.bytes.available_permits()
    }
    /// Reserve both limits for live mail, in-flight history, or queued output.
    /// Failure never permits a caller to advance a delivery floor.
    pub fn reserve(&self, bytes: usize) -> Option<Reservation> {
        let frames = self.frames.clone().try_acquire_owned().ok()?;
        let bytes = self
            .bytes
            .clone()
            .try_acquire_many_owned(bytes.try_into().ok()?)
            .ok()?;
        Some(Reservation {
            bytes: Some(bytes),
            frames: Some(frames),
            wake: self.wake.clone(),
        })
    }
}

pub(super) struct Reservation {
    bytes: Option<OwnedSemaphorePermit>,
    frames: Option<OwnedSemaphorePermit>,
    wake: Arc<Notify>,
}
impl Reservation {
    /// Return unused conservative byte capacity after actual protobuf encoding.
    pub fn shrink(&mut self, bytes: usize) {
        if let Some(permit) = &mut self.bytes {
            let unused = permit.num_permits().saturating_sub(bytes);
            drop(permit.split(unused));
        }
    }
}
impl Drop for Reservation {
    fn drop(&mut self) {
        drop(self.bytes.take());
        drop(self.frames.take());
        self.wake.notify_one();
    }
}

pub(super) enum WireResponse {
    Native(api::SubscribeResponse),
    Static(api::SubscribeStaticResponse),
}
impl WireResponse {
    pub fn encoded_len(&self) -> usize {
        match self {
            Self::Native(value) => value.encoded_len(),
            Self::Static(value) => value.encoded_len(),
        }
    }
}
pub(super) struct Frame {
    pub value: WireResponse,
    pub reservation: Reservation,
    pub challenge: Option<tokio::sync::oneshot::Sender<xmtp_common::time::Instant>>,
}

pub(super) struct SessionOutput {
    pub(super) receiver: mpsc::Receiver<Frame>,
    pub(super) terminal: Arc<Terminal>,
    ended: bool,
    task: tokio::task::JoinHandle<()>,
}

impl SessionOutput {
    pub(super) fn new(
        receiver: mpsc::Receiver<Frame>,
        terminal: Arc<Terminal>,
        task: tokio::task::JoinHandle<()>,
    ) -> Self {
        Self {
            receiver,
            terminal,
            ended: false,
            task,
        }
    }
}

impl Stream for SessionOutput {
    type Item = Result<WireResponse, Status>;
    /// Transport polling is the Ping handoff boundary. Release queue capacity
    /// here and start its response deadline, not when the owner queues the Ping.
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.ended {
            return Poll::Ready(None);
        }
        self.terminal.waker.register(cx.waker());
        if let Some(error) = self.terminal.error() {
            self.ended = true;
            return Poll::Ready(Some(Err(error)));
        }
        match self.receiver.poll_recv(cx) {
            Poll::Ready(Some(frame)) => {
                if let Some(sent) = frame.challenge {
                    let _ = sent.send(xmtp_common::time::Instant::now());
                }
                drop(frame.reservation);
                Poll::Ready(Some(Ok(frame.value)))
            }
            Poll::Ready(None) => match Pin::new(&mut self.task).poll(cx) {
                Poll::Ready(Err(_)) => {
                    self.ended = true;
                    Poll::Ready(Some(Err(Status::unavailable("session task failed"))))
                }
                Poll::Ready(Ok(())) => {
                    self.ended = true;
                    Poll::Ready(None)
                }
                Poll::Pending => Poll::Pending,
            },
            Poll::Pending => Poll::Pending,
        }
    }
}
impl Drop for SessionOutput {
    fn drop(&mut self) {
        self.task.abort();
        self.terminal.closed.store(true, Ordering::Release);
        self.terminal.wake.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[xmtp_common::test(unwrap_try = true)]
    async fn challenge_clock_starts_at_transport_poll_and_releases_queue_capacity() {
        let budget = Budget::default();
        let terminal = Arc::new(Terminal::default());
        let (sender, receiver) = mpsc::channel(1);
        let (challenge, mut handed) = tokio::sync::oneshot::channel();
        let reservation = budget.reserve(100).unwrap();
        sender
            .send(Frame {
                value: WireResponse::Native(api::SubscribeResponse {
                    response: Some(api::subscribe_response::Response::Ping(api::Ping {
                        nonce: 1,
                    })),
                }),
                reservation,
                challenge: Some(challenge),
            })
            .await
            .ok();
        assert!(matches!(
            handed.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        assert_eq!(budget.available(), OUTBOUND_QUEUE_BYTES - 100);
        let task = tokio::spawn(std::future::pending());
        let mut output = SessionOutput::new(receiver, terminal, task);
        let before_poll = xmtp_common::time::Instant::now();
        assert!(output.next().await.unwrap().is_ok());
        assert!(handed.await? >= before_poll);
        assert_eq!(budget.available(), OUTBOUND_QUEUE_BYTES);
    }
}
