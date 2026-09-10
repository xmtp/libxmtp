//! A private TCP proxy for one scenario peer.

use std::{io, net::SocketAddr};

use color_eyre::eyre::{Result, bail, eyre};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot, watch},
    task::{JoinHandle, JoinSet},
};

use crate::args::BackendOpts;

const MAX_CONNECTIONS: usize = 16;

#[derive(Clone, Copy, Default)]
struct Status {
    refused: u64,
}

enum Command {
    Pause(oneshot::Sender<(usize, u64)>),
    Resume(oneshot::Sender<()>),
}

pub(super) struct TcpProxy {
    backend: BackendOpts,
    commands: mpsc::Sender<Command>,
    status: watch::Receiver<Status>,
    task: JoinHandle<io::Result<()>>,
}

impl TcpProxy {
    pub(super) async fn start(backend: &BackendOpts) -> Result<Self> {
        if backend.url.scheme() != "http" {
            bail!("durable-streams requires a local HTTP backend for its TCP outage check");
        }
        let upstream = backend.url.socket_addrs(|| Some(80))?;
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let mut backend = backend.clone();
        backend
            .url
            .set_ip_host(address.ip())
            .map_err(|_| eyre!("cannot set the private proxy host"))?;
        backend
            .url
            .set_port(Some(address.port()))
            .map_err(|_| eyre!("cannot set the private proxy port"))?;
        let (commands, command_rx) = mpsc::channel(2);
        let (status_tx, status) = watch::channel(Status::default());
        let task = tokio::spawn(serve(listener, upstream, command_rx, status_tx));
        Ok(Self {
            backend,
            commands,
            status,
            task,
        })
    }

    pub(super) fn backend(&self) -> &BackendOpts {
        &self.backend
    }

    /// Close existing connections and reject new connections before returning.
    pub(super) async fn pause(&self) -> Result<u64> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Pause(reply))
            .await
            .map_err(|_| eyre!("private proxy stopped before the outage"))?;
        let (closed, refused) = response.await?;
        if closed == 0 {
            bail!("private proxy had no live connection to interrupt");
        }
        Ok(refused)
    }

    /// Wait for a real reconnect attempt during the outage.
    pub(super) async fn wait_for_refusal_after(&self, previous: u64) -> Result<()> {
        let mut status = self.status.clone();
        loop {
            if status.borrow().refused > previous {
                return Ok(());
            }
            status.changed().await?;
        }
    }

    pub(super) async fn resume(&self) -> Result<()> {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Resume(reply))
            .await
            .map_err(|_| eyre!("private proxy stopped during the outage"))?;
        response.await?;
        Ok(())
    }
}

impl Drop for TcpProxy {
    fn drop(&mut self) {
        // Dropping the server's JoinSet also stops all connection tasks.
        self.task.abort();
    }
}

async fn serve(
    listener: TcpListener,
    upstream: Vec<SocketAddr>,
    mut commands: mpsc::Receiver<Command>,
    status: watch::Sender<Status>,
) -> io::Result<()> {
    let mut paused = false;
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            biased;
            command = commands.recv() => match command {
                Some(Command::Pause(reply)) => {
                    paused = true;
                    connections.abort_all();
                    let mut closed = 0;
                    while let Some(result) = connections.join_next().await {
                        closed += usize::from(result.is_err_and(|error| error.is_cancelled()));
                    }
                    let _ = reply.send((closed, status.borrow().refused));
                }
                Some(Command::Resume(reply)) => {
                    paused = false;
                    let _ = reply.send(());
                }
                None => return Ok(()),
            },
            accepted = listener.accept() => {
                let (mut downstream, _) = accepted?;
                if paused || connections.len() >= MAX_CONNECTIONS {
                    drop(downstream);
                    status.send_modify(|status| status.refused += 1);
                    continue;
                }
                let upstream = upstream.clone();
                connections.spawn(async move {
                    let mut upstream = TcpStream::connect(upstream.as_slice()).await?;
                    copy_bidirectional(&mut downstream, &mut upstream).await?;
                    Ok::<_, io::Error>(())
                });
            }
            _ = connections.join_next(), if !connections.is_empty() => {}
        }
    }
}
