use anyhow::Result;
use clap::Parser;
use futures::{StreamExt, TryStreamExt, future::join_all};
use parking_lot::Mutex;
use rand::{Rng, SeedableRng, rngs::StdRng};
use ratatui::{
    DefaultTerminal, Frame,
    crossterm::event::{self, Event, KeyCode},
    layout::{Constraint, Layout, Rect},
    style::{Color, Style, Stylize},
    symbols::Marker,
    widgets::{self, Axis, Dataset, GraphType, LegendPosition},
};
use rlimit::{Resource, setrlimit};
use std::{
    collections::HashMap,
    f64,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tokio::{
    fs,
    runtime::Runtime,
    sync::{
        Mutex as TokioMutex, Notify,
        oneshot::{self, Sender},
    },
    task::JoinHandle,
    time::{sleep, timeout},
};
use tracing::{error, info};
use tui_logger::{
    LevelFilter, TuiLoggerFile, TuiLoggerLevelOutput, TuiLoggerWidget, TuiWidgetState, init_logger,
    set_default_level, set_env_filter_from_string, set_log_file,
};
use xmtp_mls::{tester, utils::Tester, xmtp_db::group_message::ContentType};

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

enum UiUpdate {
    MessageReceiveNs(u64),
    Gauge((Gauge, Option<u64>)),
}

struct ExpAvg {
    avg: f64,
    alpha: f64, // smoothing factor, typically 0.1 to 0.3
}

impl ExpAvg {
    fn new(alpha: f64) -> Self {
        Self { avg: 0.0, alpha }
    }

    fn add(&mut self, value: f64) {
        if self.avg == 0.0 {
            self.avg = value;
        } else {
            self.avg = self.alpha * value + (1.0 - self.alpha) * self.avg;
        }
    }
}

struct App {
    terminal_rx: mpsc::Receiver<UiUpdate>,
    data: Data,
    config: Config,

    ctx: Arc<Context>,
}

struct Config {
    // Delay between welcomes
    send_welcomes: Option<u64>,
}

struct Data {
    rx_process_time: Vec<u64>,
    rx_process_running_average: ExpAvg,
    rx_process_time_rolling_average: Vec<u64>,
    rx_process_rolling_last_push: Instant,
    gauges: HashMap<Gauge, u64>,
}

#[derive(Hash, PartialEq, Eq, Debug)]
enum Gauge {
    Registering,
    SendingMessages,
}

struct Context {
    args: Args,
    barrier: TokioMutex<()>,
    msg_rx: AtomicU64,
    num_registered: AtomicU64,
    num_sent: AtomicU64,
    receive_duration: Mutex<Option<Duration>>,
    ui: mpsc::Sender<UiUpdate>,
}

impl Context {
    fn total(&self) -> u64 {
        self.args.count * self.args.senders
    }
}

const RLIMIT: u64 = 4096;

fn main() -> Result<()> {
    color_eyre::install().unwrap();

    //tracing_subscriber::fmt()
    //    // filter spans/events with level TRACE or higher.
    //    // .with_max_level(Level::TRACE)
    //    .with_env_filter(EnvFilter::new("streaming=trace"))
    //    // build but do not install the subscriber.
    //    .init();

    init_logger(LevelFilter::Trace)?;
    set_default_level(LevelFilter::Trace);
    set_env_filter_from_string("streaming=trace");

    let log_file = TuiLoggerFile::new("streaming.log");
    set_log_file(log_file);

    let args = Args::parse();

    info!("Temporarily increasing the file descriptor limit to {RLIMIT}");
    setrlimit(Resource::NOFILE, RLIMIT, RLIMIT).expect("Failed to set file descriptor limit");

    let (terminal_tx, terminal_rx) = mpsc::channel();

    let mut app = App {
        terminal_rx,
        data: Data {
            gauges: HashMap::default(),
            rx_process_rolling_last_push: Instant::now(),
            rx_process_running_average: ExpAvg::new(0.001),
            rx_process_time: vec![],
            rx_process_time_rolling_average: vec![],
        },
        config: Config {
            send_welcomes: None,
        },
        ctx: Arc::new(Context {
            args,
            barrier: TokioMutex::default(),
            msg_rx: AtomicU64::default(),
            num_sent: AtomicU64::default(),
            num_registered: AtomicU64::default(),
            receive_duration: Mutex::default(),
            ui: terminal_tx,
        }),
    };

    let ctx = app.ctx.clone();
    std::thread::spawn(move || {
        let runtime = Runtime::new().unwrap();
        runtime
            .block_on(async { tokio::spawn(benchmark(ctx)).await })
            .unwrap()
            .unwrap();
    });

    let mut terminal = ratatui::init();
    app.run(&mut terminal)?;

    Ok(())
}

impl App {
    fn run(&mut self, t: &mut DefaultTerminal) -> Result<()> {
        let frame_timeout = Duration::from_secs_f64(1.0 / 60.0);

        loop {
            self.tick();

            // t.clear()?;
            t.draw(|f| {
                self.render(f);
            })?;

            if event::poll(frame_timeout)? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') {
                        return Ok(());
                    }
                }
            }
        }
    }

    fn tick(&mut self) {
        while let Ok(item) = self.terminal_rx.try_recv() {
            match item {
                UiUpdate::MessageReceiveNs(rx_process_ns) => {
                    self.data.rx_process_time.push(rx_process_ns);

                    self.data
                        .rx_process_running_average
                        .add(rx_process_ns as f64);

                    if self.data.rx_process_rolling_last_push.elapsed() > Duration::from_millis(20)
                    {
                        self.data.rx_process_rolling_last_push = Instant::now();
                        self.data
                            .rx_process_time_rolling_average
                            .push(self.data.rx_process_running_average.avg as u64);
                    }
                }
                UiUpdate::Gauge((gauge, percent)) => match percent {
                    Some(percent) => {
                        self.data.gauges.insert(gauge, percent);
                    }
                    None => {
                        self.data.gauges.remove(&gauge);
                    }
                },
            }
        }
    }

    fn render(&mut self, f: &mut Frame) {
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
                .areas(f.area());
        let [rolling_avg_rect, process_time_rect, gauges_rect] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Fill(1),
            Constraint::Length(self.data.gauges.len() as u16),
        ])
        .areas(left);

        self.render_rx_process_time(f, process_time_rect);
        self.render_rolling_avg(f, rolling_avg_rect);

        self.render_logs(f, right);
        self.render_gauges(f, gauges_rect);
    }

    fn render_gauges(&mut self, f: &mut Frame, area: Rect) {
        for (gauge, percent) in &self.data.gauges {
            let gauge = widgets::Gauge::default()
                .on_black()
                .bold()
                .label(format!("{gauge:?}"))
                .percent(*percent as u16);
            f.render_widget(gauge, area);
        }
    }

    fn render_rolling_avg(&mut self, f: &mut Frame, area: Rect) {
        let data_start = self
            .data
            .rx_process_time_rolling_average
            .len()
            .saturating_sub(area.width as usize);
        let mut data = vec![];

        let mut min = f64::MAX;
        let mut max = f64::MIN;

        for (i, val) in self.data.rx_process_time_rolling_average[data_start..]
            .iter()
            .enumerate()
        {
            let val = *val as f64;

            min = min.min(val);
            max = max.max(val);

            data.push((i as f64, val));
        }

        let mut last_val = *self
            .data
            .rx_process_time_rolling_average
            .last()
            .unwrap_or(&0) as f64;
        last_val = last_val / 1_000_000.; // milliseconds

        let datasets = vec![
            Dataset::default()
                .name(format!("ExpAvg: {last_val:.3}ms"))
                .marker(Marker::Braille)
                .style(Style::default().fg(Color::Yellow))
                .graph_type(GraphType::Line)
                .data(&data),
        ];

        let chart = widgets::Chart::new(datasets)
            .block(widgets::Block::bordered())
            .x_axis(
                Axis::default()
                    .title("X Axis")
                    .style(Style::default().gray())
                    .bounds([0.0, area.width as f64]),
            )
            .y_axis(
                Axis::default()
                    .title("Y Axis")
                    .style(Style::default().gray())
                    .bounds([min, max]),
            )
            .legend_position(Some(LegendPosition::TopLeft))
            .hidden_legend_constraints((Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)));

        f.render_widget(chart, area);
    }

    fn render_rx_process_time(&mut self, f: &mut Frame, area: Rect) {
        let start = self
            .data
            .rx_process_time
            .len()
            .saturating_sub(area.width as usize);
        let sparkline = widgets::Sparkline::default()
            .data(&self.data.rx_process_time[start..])
            .style(Color::Cyan);
        f.render_widget(sparkline, area);
    }

    fn render_logs(&mut self, f: &mut Frame, area: Rect) {
        let filter_state = TuiWidgetState::new()
            .set_default_display_level(LevelFilter::Off)
            .set_level_for_target("App", LevelFilter::Debug)
            .set_level_for_target("streaming", LevelFilter::Info);
        let widget = TuiLoggerWidget::default()
            .style_error(Style::default().fg(Color::Red))
            .style_debug(Style::default().fg(Color::Green))
            .style_warn(Style::default().fg(Color::Yellow))
            .style_trace(Style::default().fg(Color::Magenta))
            .style_info(Style::default().fg(Color::Cyan))
            .block(widgets::Block::bordered().title("Logs"))
            .output_separator('|')
            .output_timestamp(None)
            .output_level(Some(TuiLoggerLevelOutput::Long))
            .output_target(false)
            .output_file(false)
            .output_line(false)
            .style(Style::default().fg(Color::White))
            .state(&filter_state);
        f.render_widget(widget, area);
    }
}

async fn benchmark(ctx: Arc<Context>) -> Result<()> {
    let (fut, monitor_ready, inbox_id) = {
        let _barrier = ctx.barrier.lock().await;
        let (inbox_id, ready) = setup_monitor(ctx.clone()).await?;
        info!("Receiver inbox_id: {inbox_id}");
        let fut = setup_send_messages(inbox_id.clone(), &ctx).await?;

        // Sleep to allow tx to send welcomes
        sleep(Duration::from_secs(1)).await;

        (fut, ready, inbox_id)
    };

    // Wait for the monitor thread to notify that the stream is ready.
    monitor_ready.notified().await;

    let mut new_welcomes_handle: Option<JoinHandle<Result<()>>> = None;
    new_welcomes_handle = Some(
        continuous_new_welcomes(
            inbox_id.clone(),
            Duration::from_millis(10),
            Duration::from_millis(100),
        )
        .await?,
    );

    info!("Sending messages...");
    let start = Instant::now();
    fut.await;
    let elapsed = start.elapsed();

    let _ = ctx.barrier.lock().await;

    let sent = ctx.num_sent.load(Ordering::SeqCst) as i64;
    let senders = ctx.args.senders;
    let received = ctx.msg_rx.load(Ordering::SeqCst) as i64;
    let dropped = sent - received;
    let tx_elapsed = elapsed.as_secs_f32();
    let tx_rate = sent as f32 / tx_elapsed;
    let mut rx_elapsed = None;
    let mut rx_rate = None;
    if let Some(rx_duration) = *ctx.receive_duration.lock() {
        let elapsed = rx_duration.as_secs_f32();
        rx_elapsed = Some(elapsed);
        rx_rate = Some(received as f32 / elapsed);
    }

    new_welcomes_handle.inspect(|h| h.abort());

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
    jitter: Duration,
) -> Result<JoinHandle<Result<()>>> {
    let jitter = jitter.as_nanos() as u64;

    let handle = tokio::spawn(async move {
        tester!(new_guy);
        let mut start = Instant::now();
        let mut rng = StdRng::from_entropy();

        loop {
            new_guy
                .create_group_with_inbox_ids(&[&inbox_id], None, None)
                .await?;

            let jitter = rng.gen_range(0..=jitter);
            let freq = freq + Duration::from_nanos(jitter as u64);
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
    let mut monitoring_start: Option<Instant> = None;
    let grace_period = Duration::from_secs(ctx.args.timeout);

    ready.notify_one();

    #[allow(unused)]
    loop {
        let processing_start = Instant::now();
        let next_result = timeout(grace_period, async {
            loop {
                let next = stream.next().await?;
                if let Ok(msg) = &next {
                    if msg.content_type != ContentType::Unknown {}
                }
                return Some(next);
            }
        })
        .await;

        let msg = match next_result {
            Ok(Some(Ok(msg))) => msg,
            Ok(Some(Err(err))) => {
                tracing::error!("{err:?}");
                break;
            }
            Ok(None) | Err(_) => break,
        };
        ctx.ui.send(UiUpdate::MessageReceiveNs(
            processing_start.elapsed().as_nanos() as u64,
        ));

        if monitoring_start.is_none() {
            monitoring_start = Some(Instant::now());
        }

        let i = ctx.msg_rx.fetch_add(1, Ordering::SeqCst) + 1;
        if i == total {
            break;
        }
    }

    if let Some(start) = monitoring_start {
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
    let _ = tokio::fs::create_dir_all("snapshots").await;

    let mut futs = vec![];
    for i in 0..ctx.args.senders {
        futs.push(create_client(i, ctx));
    }
    let testers: Vec<Tester> = futures::stream::iter(futs)
        .buffer_unordered(100)
        .try_collect()
        .await?;

    let futs: Vec<_> = futures::stream::iter(testers)
        .map(|tester| {
            let inbox_id = inbox_id.clone();
            async move { send_messages(tester, inbox_id, ctx.clone()).await }
        })
        .buffer_unordered(100)
        .try_collect()
        .await?;

    let _ = ctx.ui.send(UiUpdate::Gauge((Gauge::Registering, None)));

    Ok(async move {
        join_all(futs).await;
        let _ = ctx.ui.send(UiUpdate::Gauge((Gauge::SendingMessages, None)));
    })
}

async fn create_client(i: u64, ctx: &Context) -> Result<Tester> {
    let snapshot_path = PathBuf::from(format!("snapshots/{i}.db3"));
    let snapshot = fs::read(&snapshot_path).await.ok().map(Arc::new);

    tester!(bo, with_dev: ctx.args.dev, ephemeral_db, with_snapshot: snapshot.clone(), disable_workers);

    if snapshot.is_none() {
        let snapshot = bo.dump_db();
        fs::write(&snapshot_path, snapshot).await?;
    }

    Ok(bo)
}

async fn send_messages(
    sender: Tester,
    inbox_id: String,
    ctx: Arc<Context>,
) -> Result<impl Future<Output = Result<()>>> {
    let dm = sender.find_or_create_dm_by_inbox_id(inbox_id, None).await?;
    let num = ctx.num_registered.fetch_add(1, Ordering::Relaxed) + 1;

    let percent = (num as f32 / ctx.args.senders as f32 * 100.) as u64;
    let _ = ctx.ui.send(UiUpdate::Gauge((
        Gauge::Registering,
        Some(percent.min(100)),
    )));

    Ok(async move {
        for i in 0..ctx.args.count {
            let num = ctx.num_sent.fetch_add(1, Ordering::SeqCst);
            let percent = (num as f32 / ctx.total() as f32 * 100.) as u64;
            let _ = ctx.ui.send(UiUpdate::Gauge((
                Gauge::SendingMessages,
                Some(percent.min(100)),
            )));

            dm.send_message(format!("{i}").as_bytes(), Default::default())
                .await?;
        }

        Ok(())
    })
}
