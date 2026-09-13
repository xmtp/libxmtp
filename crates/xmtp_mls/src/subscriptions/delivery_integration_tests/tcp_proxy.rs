//! A private TCP fault for one reader. Other tests keep their own connections.

use std::{io, net::SocketAddr};
use tokio::{
    io::copy_bidirectional,
    net::{TcpListener, TcpStream},
    sync::{mpsc, oneshot, watch},
    task::{JoinHandle, JoinSet},
};

enum Command {
    Pause(oneshot::Sender<usize>),
    Resume(oneshot::Sender<()>),
}

pub(super) struct TcpProxy {
    pub address: SocketAddr,
    commands: mpsc::Sender<Command>,
    refused: watch::Receiver<u64>,
    task: JoinHandle<io::Result<()>>,
}

impl TcpProxy {
    pub async fn start(upstream: Vec<SocketAddr>) -> io::Result<Self> {
        let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let (commands, command_rx) = mpsc::channel(2);
        let (refused_tx, refused) = watch::channel(0);
        let task = tokio::spawn(serve(listener, upstream, command_rx, refused_tx));
        Ok(Self {
            address,
            commands,
            refused,
            task,
        })
    }

    pub async fn pause(&self) -> u64 {
        let previous = *self.refused.borrow();
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Pause(reply))
            .await
            .expect("proxy accepts pause");
        assert!(
            response.await.expect("proxy confirms closed connections") > 0,
            "the outage must close an active connection"
        );
        previous
    }

    pub async fn wait_for_refusal_after(&self, previous: u64) {
        let mut refused = self.refused.clone();
        while *refused.borrow() <= previous {
            refused
                .changed()
                .await
                .expect("proxy reports reconnect attempts");
        }
    }

    pub async fn resume(&self) {
        let (reply, response) = oneshot::channel();
        self.commands
            .send(Command::Resume(reply))
            .await
            .expect("proxy accepts resume");
        response.await.expect("proxy confirms resume");
    }
}

impl Drop for TcpProxy {
    fn drop(&mut self) {
        // Dropping the server's JoinSet also stops its connection tasks.
        self.task.abort();
    }
}

async fn serve(
    listener: TcpListener,
    upstream: Vec<SocketAddr>,
    mut commands: mpsc::Receiver<Command>,
    refused: watch::Sender<u64>,
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
                    let _ = reply.send(closed);
                }
                Some(Command::Resume(reply)) => {
                    paused = false;
                    let _ = reply.send(());
                }
                None => return Ok(()),
            },
            accepted = listener.accept() => {
                let (mut downstream, _) = accepted?;
                if paused || connections.len() >= 16 {
                    drop(downstream);
                    refused.send_modify(|count| *count += 1);
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
