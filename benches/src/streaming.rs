use anyhow::Result;
use clap::Parser;
use futures::{StreamExt, TryStreamExt, future::join_all};
use indicatif::ProgressBar;
use parking_lot::Mutex;
use rlimit::{Resource, setrlimit};
use std::{
    path::PathBuf,
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
    task::JoinHandle,
    time::{sleep, timeout},
};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;
use xmtp_mls::{tester, utils::Tester};

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

const RLIMIT: u64 = 4096;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        // filter spans/events with level TRACE or higher.
        // .with_max_level(Level::TRACE)
        .with_env_filter(EnvFilter::new("streaming=trace"))
        // build but do not install the subscriber.
        .init();

    let args = Args::parse();

    info!("Temporarily increasing the file descriptor limit to {RLIMIT}");
    setrlimit(Resource::NOFILE, RLIMIT, RLIMIT).expect("Failed to set file descriptor limit");

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

    let (fut, monitor_ready, inbox_id) = {
        let _barrier = app.ctx.barrier.lock().await;
        let (inbox_id, ready) = setup_monitor(app.ctx.clone()).await?;
        info!("Receiver inbox_id: {inbox_id}");
        let fut = setup_send_messages(inbox_id.clone(), &app.ctx).await?;

        // Sleep to allow tx to send welcomes
        sleep(Duration::from_secs(1)).await;

        (fut, ready, inbox_id)
    };

    // Wait for the monitor thread to notify that the stream is ready.
    monitor_ready.notified().await;

    let new_welcomes_handle =
        continuous_new_welcomes(inbox_id.clone(), Duration::from_millis(100)).await?;

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

    new_welcomes_handle.abort();

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

async fn continuous_new_welcomes(
    inbox_id: String,
    freq: Duration,
) -> Result<JoinHandle<Result<()>>> {
    let handle = tokio::spawn(async move {
        tester!(new_guy);
        let mut start = Instant::now();
        loop {
            new_guy
                .create_group_with_inbox_ids(&[&inbox_id], None, None)
                .await?;
            info!("Sent welcome");
            tokio::time::sleep(freq.saturating_sub(start.elapsed())).await;
            start = Instant::now();
        }

        #[allow(unreachable_code)]
        Ok(())
    });

    Ok(handle)
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
    tester!(andre, with_dev: ctx.args.dev, disable_workers);
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
    while let Ok(Some(msg)) = timeout(grace_period, stream.next()).await {
        let msg = match msg {
            Ok(msg) => msg,
            Err(err) => {
                tracing::error!("{err:?}");
                break;
            }
        };

        if start.is_none() {
            start = Some(Instant::now());
        }

        let i = ctx.msg_rx.fetch_add(1, Ordering::SeqCst) + 1;
        if i == total {
            break;
        }
    }

    if let Some(start) = start {
        let elapsed = start.elapsed();
        *ctx.receive_duration.lock() = Some(elapsed);
    }

    Ok(())
}

async fn setup_send_messages(
    inbox_id: String,
    ctx: &Arc<Context>,
) -> Result<impl Future<Output = ()>> {
    info!("Registering {} senders...", ctx.args.senders);
    let mut progress = None;
    if ctx.args.senders > 10 {
        progress = Some(ProgressBar::new(ctx.args.senders));
    }

    let _ = tokio::fs::create_dir_all("snapshots").await;

    let mut futs = vec![];
    for i in 0..ctx.args.senders {
        futs.push(create_client(i, ctx, progress.clone()));
    }
    let testers: Vec<Tester> = futures::stream::iter(futs)
        .buffer_unordered(100)
        .try_collect()
        .await?;
    progress.inspect(|p| p.finish());

    let futs: Vec<_> = futures::stream::iter(testers)
        .map(|tester| {
            let inbox_id = inbox_id.clone();
            async move { send_messages(tester, inbox_id, ctx.clone()).await }
        })
        .buffer_unordered(100)
        .try_collect()
        .await?;

    Ok(async move {
        join_all(futs).await;
    })
}

async fn create_client(
    i: u64,
    ctx: &Context,
    register_progress: Option<ProgressBar>,
) -> Result<Tester> {
    let snapshot_path = PathBuf::from(format!("snapshots/{i}.db3"));
    let snapshot = fs::read(&snapshot_path).await.ok().map(Arc::new);

    tester!(bo, with_dev: ctx.args.dev, ephemeral_db, with_snapshot: snapshot.clone(), disable_workers);

    if snapshot.is_none() {
        let snapshot = bo.dump_db();
        fs::write(&snapshot_path, snapshot).await?;
    }

    if let Some(progress) = register_progress {
        progress.inc(1);
    }

    Ok(bo)
}

async fn send_messages(
    sender: Tester,
    inbox_id: String,
    ctx: Arc<Context>,
) -> Result<impl Future<Output = Result<()>>> {
    let dm = sender.find_or_create_dm_by_inbox_id(inbox_id, None).await?;

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
