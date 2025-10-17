use anyhow::Result;
use clap::Parser;
use futures::{StreamExt, future::join_all};
use indicatif::ProgressBar;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    sync::{
        Mutex,
        oneshot::{self, Sender},
    },
    time::{sleep, timeout},
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use xmtp_mls::tester;

#[derive(Parser)]
struct Args {
    #[arg(short, long, default_value = "100")]
    count: u64,
    #[arg(short, long, default_value = "10")]
    senders: u64,
    #[arg(short, long, default_value = "2")]
    timeout: u64,
}

struct App {
    ctx: Arc<Context>,
}

struct Context {
    args: Args,
    tx_progress: ProgressBar,
    barrier: Mutex<()>,
    msg_rx: AtomicU64,
    msg_tx: AtomicU64,
}

impl Context {
    fn total(&self) -> u64 {
        self.args.count * self.args.senders
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        // filter spans/events with level TRACE or higher.
        // .with_max_level(Level::TRACE)
        .with_env_filter(EnvFilter::new("streaming=trace"))
        // build but do not install the subscriber.
        .init();

    let args = Args::parse();
    let num_msgs = args.senders * args.count;
    let app = App {
        ctx: Arc::new(Context {
            args,
            tx_progress: ProgressBar::new(num_msgs),
            barrier: Mutex::default(),
            msg_rx: AtomicU64::default(),
            msg_tx: AtomicU64::default(),
        }),
    };

    let fut = {
        let _barrier = app.ctx.barrier.lock().await;
        let inbox_id = setup_monitor(app.ctx.clone()).await?;
        info!("inbox_id: {inbox_id}");
        let fut = setup_send_messages(inbox_id.clone(), &app.ctx).await?;

        // Sleep to allow tx to send welcomes
        sleep(Duration::from_secs(1)).await;

        fut
    };

    // Sleep to allow rx to receive welcomes
    sleep(Duration::from_secs(1)).await;

    let start = Instant::now();
    fut.await;
    let elapsed = start.elapsed();

    let _ = app.ctx.barrier.lock().await;

    info!(
        "{} messages sent, {} messages received",
        app.ctx.msg_tx.load(Ordering::SeqCst),
        app.ctx.msg_rx.load(Ordering::SeqCst)
    );

    info!(
        "\n{} messages\nfrom {} senders\nin {} seconds",
        app.ctx.args.count,
        app.ctx.args.senders,
        elapsed.as_secs_f32()
    );

    Ok(())
}

async fn setup_monitor(ctx: Arc<Context>) -> Result<String> {
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        if let Err(err) = monitor_messages(tx, ctx).await {
            error!("{err:?}");
        };
    });

    Ok(rx.await?)
}

async fn monitor_messages(tx: Sender<String>, ctx: Arc<Context>) -> Result<()> {
    tester!(andre);
    tx.send(andre.inbox_id().to_string())
        .expect("Failed to share inbox_id");

    // This barrier will wait for the senders to send their welcomes.
    let _barrier = ctx.barrier.lock().await;
    let groups = andre.sync_welcomes().await?;
    info!("Received welcomes into {} groups", groups.len());

    let total = ctx.total();

    let mut stream = andre.stream_all_messages(None, None).await?;
    #[allow(unused)]
    while let Some(Ok(msg)) = timeout(Duration::from_secs(ctx.args.timeout), stream.next())
        .await
        .inspect_err(|_| error!("Timed out"))?
    {
        // let msg = String::from_utf8_lossy(&msg.decrypted_message_bytes);
        // info!("{msg}");

        let i = ctx.msg_rx.fetch_add(1, Ordering::SeqCst) + 1;
        if i == total {
            break;
        }
    }

    Ok(())
}

async fn setup_send_messages(
    inbox_id: String,
    ctx: &Arc<Context>,
) -> Result<impl Future<Output = ()>> {
    let mut futs = Vec::with_capacity(ctx.args.senders as usize);
    for _ in 0..ctx.args.senders {
        futs.push(send_messages(inbox_id.clone(), ctx.clone()).await?);
    }

    Ok(async move {
        join_all(futs).await;
    })
}

async fn send_messages(
    inbox_id: String,
    ctx: Arc<Context>,
) -> Result<impl Future<Output = Result<()>>> {
    tester!(bodashery);
    let dm = bodashery
        .find_or_create_dm_by_inbox_id(inbox_id, None)
        .await?;

    Ok(async move {
        for i in 0..ctx.args.count {
            ctx.msg_tx.fetch_add(1, Ordering::SeqCst);
            ctx.tx_progress.inc(1);
            dm.send_message(format!("{i}").as_bytes(), Default::default())
                .await?;
        }

        Ok(())
    })
}
