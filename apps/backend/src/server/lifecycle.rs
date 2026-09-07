//! Admission and cancellation for one serving instance.

use futures::{FutureExt, future::BoxFuture};
use http::{Request, Response};
use std::{
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpStream,
    sync::watch,
};
use tonic::{body::Body, transport::server::Connected};
use tower::{Layer, Service};

pub(super) struct Lifecycle {
    stopping: AtomicBool,
    canceled: watch::Sender<bool>,
}

impl Lifecycle {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            stopping: AtomicBool::new(false),
            canceled: watch::channel(false).0,
        })
    }

    /// Close admission before stopping listeners or failing existing streams.
    pub fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    /// Cancel handlers and socket IO at the end of the unary drain budget.
    pub fn cancel(&self) {
        self.stop();
        self.canceled.send_replace(true);
    }

    fn deadline(&self) -> BoxFuture<'static, ()> {
        let mut canceled = self.canceled.subscribe();
        async move {
            loop {
                if *canceled.borrow_and_update() {
                    return;
                }
                if canceled.changed().await.is_err() {
                    return;
                }
            }
        }
        .boxed()
    }

    /// Give each accepted connection a deadline wakeup, including idle sockets
    /// and responses blocked by peer flow control. Tonic spawns connection tasks
    /// separately, so dropping its listener future alone does not cancel them.
    pub fn connection(&self, inner: TcpStream) -> DrainIo {
        DrainIo {
            inner,
            deadline: self.deadline(),
            canceled: false,
        }
    }
}

#[derive(Clone)]
pub(super) struct AdmissionLayer(pub Arc<Lifecycle>);
impl<S> Layer<S> for AdmissionLayer {
    type Service = Admission<S>;
    fn layer(&self, inner: S) -> Self::Service {
        Admission {
            inner,
            lifecycle: self.0.clone(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Admission<S> {
    inner: S,
    lifecycle: Arc<Lifecycle>,
}
impl<S> Service<Request<Body>> for Admission<S>
where
    S: Service<Request<Body>, Response = Response<Body>>,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.lifecycle.stopping.load(Ordering::Acquire) {
            Poll::Ready(Ok(()))
        } else {
            self.inner.poll_ready(cx)
        }
    }

    /// Requests admitted before shutdown get the remaining drain budget. Later
    /// requests fail without reaching payload validation or database code.
    fn call(&mut self, request: Request<Body>) -> Self::Future {
        if self.lifecycle.stopping.load(Ordering::Acquire) {
            return Box::pin(async { Ok(unavailable()) });
        }
        let future = self.inner.call(request);
        let deadline = self.lifecycle.deadline();
        Box::pin(async move {
            tokio::select! { biased; _ = deadline => Ok(unavailable()), result = future => result }
        })
    }
}

fn unavailable() -> Response<Body> {
    tonic::Status::unavailable("server is shutting down").into_http()
}

pub(super) struct DrainIo {
    inner: TcpStream,
    deadline: BoxFuture<'static, ()>,
    canceled: bool,
}
impl DrainIo {
    fn check(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        if !self.canceled {
            self.canceled = self.deadline.as_mut().poll(cx).is_ready();
        }
        if self.canceled {
            Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "server drain deadline reached",
            ))
        } else {
            Ok(())
        }
    }
}
impl Connected for DrainIo {
    type ConnectInfo = <TcpStream as Connected>::ConnectInfo;
    fn connect_info(&self) -> Self::ConnectInfo {
        self.inner.connect_info()
    }
}
impl AsyncRead for DrainIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.check(cx)?;
        Pin::new(&mut self.inner).poll_read(cx, buffer)
    }
}
impl AsyncWrite for DrainIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.check(cx)?;
        Pin::new(&mut self.inner).poll_write(cx, bytes)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.check(cx)?;
        Pin::new(&mut self.inner).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

pub(super) struct ShutdownGuard {
    pub lifecycle: Arc<Lifecycle>,
    pub streams: Option<Arc<crate::stream::StreamHub>>,
}
impl ShutdownGuard {
    pub fn stop(&self) {
        self.lifecycle.stop();
        if let Some(streams) = &self.streams {
            streams.stop();
        }
    }
}
impl Drop for ShutdownGuard {
    fn drop(&mut self) {
        self.stop();
        self.lifecycle.cancel();
    }
}
