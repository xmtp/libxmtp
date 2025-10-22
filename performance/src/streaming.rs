use anyhow::Result;
use clap::Parser;
use futures::{StreamExt, TryStreamExt, future::join_all};
use indicatif::ProgressBar;
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    fs,
    sync::{
        Mutex as TokioMutex, Notify,
        oneshot::{self, Sender},
    },
    time::{sleep, timeout},
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use xmtp_mls::{identity::Identity, tester, xmtp_db::identity::StoredIdentity};

#[derive(Parser)]
struct Args {
    #[arg(short, long, default_value = "100")]
    count: u64,
    #[arg(short, long, default_value = "10")]
    senders: u64,
    #[arg(short, long, default_value = "2")]
    timeout: u64,
    #[arg(short, long, default_value = "false")]
    dev: bool,
}

struct App {
    ctx: Arc<Context>,
}

struct Context {
    args: Args,
    tx_progress: ProgressBar,
    barrier: TokioMutex<()>,
    msg_rx: AtomicU64,
    msg_tx: AtomicU64,
    receive_duration: Mutex<Option<Duration>>,
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
            barrier: TokioMutex::default(),
            msg_rx: AtomicU64::default(),
            msg_tx: AtomicU64::default(),
            receive_duration: Mutex::default(),
        }),
    };

    let (fut, monitor_ready) = {
        let _barrier = app.ctx.barrier.lock().await;
        let (inbox_id, ready) = setup_monitor(app.ctx.clone()).await?;
        info!("Receiver inbox_id: {inbox_id}");
        let fut = setup_send_messages(inbox_id.clone(), &app.ctx).await?;

        // Sleep to allow tx to send welcomes
        sleep(Duration::from_secs(1)).await;

        (fut, ready)
    };

    // Wait for the monitor thread to notify that the stream is ready.
    monitor_ready.notified().await;

    info!("Sending messages...");
    let start = Instant::now();
    fut.await;
    let elapsed = start.elapsed();
    app.ctx.tx_progress.finish();

    let _ = app.ctx.barrier.lock().await;

    let sent = app.ctx.msg_tx.load(Ordering::SeqCst) as i64;
    let senders = app.ctx.args.senders;
    let received = app.ctx.msg_rx.load(Ordering::SeqCst) as i64;
    let dropped = sent - received;
    let tx_elapsed = elapsed.as_secs_f32();
    let tx_rate = sent as f32 / tx_elapsed;
    let mut rx_elapsed = None;
    let mut rx_rate = None;
    if let Some(rx_duration) = *app.ctx.receive_duration.lock() {
        let elapsed = rx_duration.as_secs_f32();
        rx_elapsed = Some(elapsed);
        rx_rate = Some(received as f32 / elapsed);
    }

    let rx_elapsed = rx_elapsed
        .map(|rx| rx.to_string())
        .unwrap_or("Unknown".to_string());
    let rx_rate = rx_rate
        .map(|rx| rx.to_string())
        .unwrap_or("Unknown".to_string());

    info!(
        "\nREPORT:\n\
        {sent} messages sent across {senders} senders,\n\
        {received} messages received ({dropped} dropped)\n\
        rx time: {rx_elapsed} seconds ({rx_rate} msgs/s)\n\
        tx time: {tx_elapsed} seconds ({tx_rate} msgs/s)",
    );

    Ok(())
}

async fn setup_monitor(ctx: Arc<Context>) -> Result<(String, Arc<Notify>)> {
    let (tx, rx) = oneshot::channel();
    let ready = Arc::new(Notify::new());
    tokio::spawn({
        let ready = ready.clone();
        async move {
            if let Err(err) = monitor_messages(tx, ctx, ready).await {
                error!("{err:?}");
            };
        }
    });

    Ok((rx.await?, ready))
}

async fn monitor_messages(tx: Sender<String>, ctx: Arc<Context>, ready: Arc<Notify>) -> Result<()> {
    tester!(andre, with_dev: ctx.args.dev);
    tx.send(andre.inbox_id().to_string())
        .expect("Failed to share inbox_id");

    // This barrier will wait for the senders to send their welcomes.
    let _barrier = ctx.barrier.lock().await;
    let groups = andre.sync_welcomes().await?;
    info!("Received welcomes into {} groups", groups.len());

    let total = ctx.total();

    let mut stream = andre.stream_all_messages(None, None).await?;
    let mut start: Option<Instant> = None;
    let grace_period = Duration::from_secs(ctx.args.timeout);

    ready.notify_one();

    #[allow(unused)]
    while let Some(Ok(msg)) = timeout(grace_period, stream.next())
        .await
        .inspect_err(|_| {
            if let Some(start) = start {
                let elapsed = start.elapsed() - grace_period;
                *ctx.receive_duration.lock() = Some(elapsed);
            }
            error!("Timed out")
        })?
    {
        if start.is_none() {
            start = Some(Instant::now());
        }
        // let msg = String::from_utf8_lossy(&msg.decrypted_message_bytes);
        // info!("{msg}");

        let i = ctx.msg_rx.fetch_add(1, Ordering::SeqCst) + 1;
        if i == total {
            if let Some(start) = start {
                let elapsed = start.elapsed();
                *ctx.receive_duration.lock() = Some(elapsed);
            }
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

    info!("Registering {} senders...", ctx.args.senders);
    let mut progress = None;
    if ctx.args.senders > 10 {
        progress = Some(ProgressBar::new(ctx.args.senders));
    }

    for _ in 0..ctx.args.senders {
        futs.push(send_messages(
            inbox_id.clone(),
            ctx.clone(),
            progress.clone(),
        ));
    }

    let futs: Vec<_> = futures::stream::iter(futs)
        .buffer_unordered(10)
        .try_collect()
        .await?;

    Ok(async move {
        join_all(futs).await;
    })
}

async fn send_messages(
    inbox_id: String,
    ctx: Arc<Context>,
    register_progress: Option<ProgressBar>,
) -> Result<impl Future<Output = Result<()>>> {
    let cached_ident = fs::read("ident").await.unwrap();
    let cached_ident: StoredIdentity = serde_json::from_slice(&cached_ident).unwrap();
    let cached_ident: Identity = cached_ident.try_into().unwrap();
    tester!(bodashery, with_dev: ctx.args.dev, ephemeral_db, external_identity: cached_ident);

    // tester!(bodashery, with_dev: ctx.args.dev);

    let dm = bodashery
        .find_or_create_dm_by_inbox_id(inbox_id, None)
        .await?;
    register_progress.inspect(|p| p.inc(1));

    info!("{:?}", bodashery.inbox_id());

    tokio::time::sleep(Duration::from_secs(1)).await;

    let identity: StoredIdentity = bodashery.identity().try_into().unwrap();
    let ident = serde_json::to_vec(&identity).unwrap();
    fs::write("ident", ident).await.unwrap();

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
