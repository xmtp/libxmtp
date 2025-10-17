use anyhow::Result;
use clap::Parser;
use futures::StreamExt;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio::{
    runtime::Runtime,
    sync::oneshot::{self, Sender},
};
use tracing::{error, info};
use xmtp_mls::tester;

#[derive(Parser)]
struct Args {}

struct App {
    ctx: Arc<Context>,
}

#[derive(Default)]
struct Context {
    msg_rx: AtomicUsize,
    msg_tx: AtomicUsize,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let app = App {
        ctx: Arc::default(),
    };

    let inbox_id = setup_monitor(app.ctx.clone())?;
    info!("Monitor inbox_id: {inbox_id}");

    Ok(())
}

fn setup_monitor(ctx: Arc<Context>) -> Result<String> {
    let (tx, rx) = oneshot::channel();

    std::thread::spawn(move || {
        let rt = Runtime::new()?;
        rt.block_on(monitor_messages(tx, ctx));

        anyhow::Ok(())
    });

    Ok(rx.blocking_recv()?)
}

async fn monitor_messages(tx: Sender<String>, ctx: Arc<Context>) -> Result<()> {
    tester!(andre);
    tx.send(andre.inbox_id().to_string());

    let mut stream = andre.stream_all_messages(None, None).await?;
    while let Some(Ok(msg)) = stream.next().await {
        ctx.msg_rx.fetch_add(1, Ordering::SeqCst);
    }

    Ok(())
}

fn setup_sender(ctx: Arc<Context>) -> Result<()> {
    std::thread::spawn(move || {
        let rt = Runtime::new()?;
        rt.block_on(async move {});

        anyhow::Ok(())
    });

    Ok(())
}
