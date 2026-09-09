use super::*;
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
};
use xmtp_common::time::{Duration, Instant, sleep, timeout};
use xmtp_logging::test_logging::OtlpCollector;

const CHILD_CONFIG: &str = "XMTP_BACKEND_STARTUP_TEST_CONFIG";
const TEST_NAME: &str = "tests::startup_preserves_metrics_export_and_shutdown_across_log_settings";
const DRAIN_MS: u64 = 1_000;

struct Process {
    child: Child,
    directory: PathBuf,
}

impl Process {
    fn start(config: &str, endpoint: Option<&str>) -> Self {
        let directory = std::env::temp_dir().join(format!("xmtp-startup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("backend.toml");
        std::fs::write(&path, config).unwrap();
        let output = std::fs::File::create(directory.join("output")).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(CHILD_CONFIG, path)
            .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output));
        if let Some(endpoint) = endpoint {
            command.env("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint);
        }
        Self {
            child: command.spawn().unwrap(),
            directory,
        }
    }

    fn output(&self) -> String {
        std::fs::read_to_string(self.directory.join("output")).unwrap()
    }

    async fn terminate(&mut self) {
        let start = Instant::now();
        assert!(
            Command::new("kill")
                .args(["-TERM", &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
        let status = timeout(
            Duration::from_millis(DRAIN_MS) + Duration::from_secs(5),
            async {
                loop {
                    if let Some(status) = self.child.try_wait().unwrap() {
                        break status;
                    }
                    sleep(Duration::from_millis(20)).await;
                }
            },
        )
        .await
        .expect("drain and export finish within the deadline");
        assert!(status.success(), "{}", self.output());
        assert!(start.elapsed() < Duration::from_millis(DRAIN_MS) + Duration::from_secs(5));
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

fn address() -> String {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .to_string()
}

async fn scrape(address: &str, expected: &str) -> String {
    let client = xmtp_common::http::client().unwrap();
    timeout(Duration::from_secs(20), async {
        loop {
            if let Ok(response) = client.get(format!("http://{address}/metrics")).send().await {
                let text = response.text().await.unwrap();
                if text.contains(expected) {
                    break text;
                }
            }
            sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("missing metric {expected}"))
}

#[xmtp_common::test(
    unwrap_try = true,
    disable_logging = true,
    flavor = "multi_thread",
    worker_threads = 4
)]
async fn startup_preserves_metrics_export_and_shutdown_across_log_settings() {
    if let Ok(path) = std::env::var(CHILD_CONFIG) {
        xmtp_cryptography::install_crypto_provider();
        let config = Config::load(path)?;
        if config.telemetry.metrics_listen.is_empty() {
            let occupied = TcpListener::bind("0.0.0.0:9464")?;
            let metrics = telemetry::install("")?;
            telemetry::ready(false);
            assert!(metrics.render().contains("xmtp_backend_ready 0"));
            drop(occupied);
        } else {
            run(config, shutdown()).await?;
        }
        return;
    }
    let database = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://xmtp:xmtp@localhost:55432/xmtp_backend".into());
    let disabled = format!("[database]\nurl = {database:?}\n[telemetry]\nmetrics_listen = ''\n");
    let mut process = Process::start(&disabled, None);
    assert!(process.child.wait()?.success(), "{}", process.output());
    for level in [
        "default",
        "json",
        "warn",
        "error",
        "unreachable",
        "unsampled",
    ] {
        let collector = OtlpCollector::start().await?;
        let grpc = address();
        let metrics = if level == "default" {
            "127.0.0.1:9464".into()
        } else {
            address()
        };
        let mut config = format!(
            "[database]\nurl = {database:?}\n[server]\nlisten = {grpc:?}\nmax_drain_duration_ms = {DRAIN_MS}\n"
        );
        if matches!(level, "warn" | "error") {
            config.push_str(&format!("log_level = {level:?}\n"));
        }
        if level == "json" {
            config.push_str("log_format = 'json'\n");
        }
        if level != "default" {
            config.push_str(&format!("[telemetry]\nmetrics_listen = {metrics:?}\n"));
        }
        if level == "unsampled" {
            config.push_str("sample_ratio = 0.0\n");
        }
        let endpoint = if level == "unreachable" {
            format!("http://{}", address())
        } else {
            collector.endpoint()
        };
        let mut process =
            Process::start(&config, (level != "default").then_some(endpoint.as_str()));
        let ready = scrape(&metrics, "xmtp_backend_ready 1").await;
        assert!(ready.contains("xmtp_backend_info{version="));
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{grpc}"))?
            .connect()
            .await?;
        let mut client = xmtp_backend::api::query_service_client::QueryServiceClient::new(channel);
        let _ = client
            .get(xmtp_backend::api::GetRequest { sequence_id: 1 })
            .await;
        scrape(&metrics, "operation=\"db.get\"").await;
        if level == "unreachable" {
            scrape(&metrics, "xmtp_telemetry_export_failures_total 1").await;
            // A failed batch must not stop the request path.
            let _ = client
                .get(xmtp_backend::api::GetRequest { sequence_id: 1 })
                .await;
            scrape(&metrics, "xmtp_backend_ready 1").await;
        }
        process.terminate().await;
        let spans = collector.take_spans();
        assert_eq!(
            spans.is_empty(),
            matches!(level, "default" | "unreachable" | "unsampled"),
            "{level}"
        );
        if matches!(level, "json" | "warn" | "error") {
            assert!(
                spans
                    .iter()
                    .flat_map(|resource| &resource.scope_spans)
                    .flat_map(|scope| &scope.spans)
                    .any(|span| span.name == "db.get"),
                "INFO database spans must export at stdout level {level}"
            );
        }
        assert!(collector.take_logs().is_empty());
        let output = process.output();
        if level == "json" {
            let line = output
                .lines()
                .find(|line| line.contains("backend ready to serve"))?;
            let event: serde_json::Value = serde_json::from_str(line)?;
            assert_eq!(event["message"], "backend ready to serve");
        } else if level == "default" {
            assert!(output.contains("backend ready to serve"));
            assert!(!output.contains("\"message\":\"backend ready to serve\""));
        }
    }

    // Keep the database handshake pending and observe readiness before initialization completes.
    let blocked_database = TcpListener::bind("127.0.0.1:0")?;
    let metrics = address();
    let config = format!(
        "[database]\nurl = 'postgres://xmtp:xmtp@{}/xmtp_backend'\n[telemetry]\nmetrics_listen = {metrics:?}\n",
        blocked_database.local_addr()?
    );
    let process = Process::start(&config, None);
    assert!(
        scrape(&metrics, "xmtp_backend_ready 0")
            .await
            .contains("xmtp_backend_info{version=")
    );
    drop(process);

    let occupied = TcpListener::bind("127.0.0.1:0")?;
    let config = format!(
        "[database]\nurl = {database:?}\n[telemetry]\nmetrics_listen = '{}'\n",
        occupied.local_addr()?
    );
    let mut process = Process::start(&config, None);
    let status = process.child.wait()?;
    assert!(!status.success());
    assert!(process.output().contains(&format!(
        "telemetry.metrics_listen {}",
        occupied.local_addr()?
    )));
}
