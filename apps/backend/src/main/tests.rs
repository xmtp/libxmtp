use super::*;
use std::{
    net::TcpListener,
    process::{Child, Command, Stdio},
};
use xmtp_common::time::{Duration, Instant, sleep, timeout};
use xmtp_logging::test_logging::OtlpCollector;

const CHILD_CONFIG: &str = "XMTP_BACKEND_STARTUP_TEST_CONFIG";
const CHILD_TEST: &str = "startup_preserves_metrics_export_and_shutdown_across_log_settings";
const CHILD_ARGS: &str = "XMTP_BACKEND_STARTUP_TEST_ARGS";
const SECRET: &str = "private-config-sentinel";
const DRAIN_MS: u64 = 1_000;

/// The libtest filter for the test that the child process runs.
fn child_test_name() -> String {
    let (_, module) = module_path!()
        .split_once("::")
        .expect("a test module below the crate root");
    format!("{module}::{CHILD_TEST}")
}

struct Process {
    child: Child,
    directory: PathBuf,
}

impl Process {
    fn start(config: &str, endpoint: Option<&str>) -> Self {
        Self::start_with_args(config, endpoint, None, None)
    }

    fn start_with_args(
        config: &str,
        endpoint: Option<&str>,
        args: Option<&[&str]>,
        inline_env: Option<&str>,
    ) -> Self {
        let directory = std::env::temp_dir().join(format!("xmtp-startup-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("backend.toml");
        std::fs::write(&path, config).unwrap();
        let output = std::fs::File::create(directory.join("output")).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", &child_test_name(), "--nocapture"])
            .env(CHILD_CONFIG, &path)
            .env_remove("XMTP_CONFIG")
            .env_remove(CHILD_ARGS)
            .env("XMTP_STARTUP_TEST_SECRET", SECRET)
            .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
            .stdout(Stdio::from(output.try_clone().unwrap()))
            .stderr(Stdio::from(output));
        if let Some(args) = args {
            command.env(CHILD_ARGS, serde_json::to_string(args).unwrap());
        }
        if let Some(contents) = inline_env {
            command.env("XMTP_CONFIG", contents);
        }
        if let Some(endpoint) = endpoint {
            command.env("OTEL_EXPORTER_OTLP_ENDPOINT", endpoint);
        }
        Self {
            child: command.spawn().unwrap(),
            directory,
        }
    }

    /// The child's output. libtest exits successfully when its filter
    /// matches no test, so the output must show that the one test ran.
    fn output(&self) -> String {
        let output = std::fs::read_to_string(self.directory.join("output")).unwrap();
        assert!(output.contains("running 1 test\n"), "{output}");
        output
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
        let arguments = match std::env::var(CHILD_ARGS) {
            Ok(arguments) => serde_json::from_str::<Vec<String>>(&arguments)?,
            Err(_) => vec!["--config-file".into(), path],
        };
        let args = Args::parse_from(std::iter::once("xmtp-backend".to_string()).chain(arguments));
        let config = args.load_config()?;
        if config.telemetry.metrics_listen.is_empty() {
            // Occupy the default metrics port, so an exporter that binds it
            // anyway fails. When another process already holds the port, it
            // is occupied too: a Darwin Nix build shares the host loopback,
            // where a local backend stack may serve metrics on 9464.
            let occupied = match TcpListener::bind("0.0.0.0:9464") {
                Ok(listener) => Some(listener),
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => None,
                Err(error) => panic!("could not bind the default metrics port: {error}"),
            };
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
    let disabled = format!(
        "[database]\nurl = {database:?}\n[server]\nidentifier = 'org.xmtp.test'\n[telemetry]\nmetrics_listen = ''\n"
    );
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
        // Every case takes a free port. The default listener port may be held by
        // the local stack, which would make this test depend on `just backend up`.
        let metrics = address();
        let mut config = format!(
            "[database]\nurl = {database:?}\n[server]\nidentifier = 'org.xmtp.test'\nlisten = {grpc:?}\nmax_drain_duration_ms = {DRAIN_MS}\n"
        );
        if matches!(level, "warn" | "error") {
            config.push_str(&format!("log_level = {level:?}\n"));
        }
        if level == "json" {
            config.push_str("log_format = 'json'\n");
        }
        // The "default" case keeps every other telemetry key absent. It still names
        // a free listener, because the documented default port may be taken by the
        // local stack. `config::tests` covers the default value itself.
        config.push_str(&format!("[telemetry]\nmetrics_listen = {metrics:?}\n"));
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
        let request = xmtp_backend::api::QueryNewestRequest {
            topics: vec![xmtp_backend::api::Topic {
                topic: xmtp_proto::types::Topic::new_group_message(
                    xmtp_proto::types::GroupId::ZERO,
                )
                .cloned_vec(),
            }],
            include_full_envelope: false,
        };
        client.query_newest(request.clone()).await?;
        scrape(&metrics, "operation=\"db.newest_metadata\"").await;
        if level == "unreachable" {
            scrape(&metrics, "xmtp_telemetry_export_failures_total 1").await;
            // A failed batch must not stop the request path.
            client.query_newest(request).await?;
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
                    .any(|span| span.name == "db.newest_metadata"),
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
        "[database]\nurl = 'postgres://xmtp:xmtp@{}/xmtp_backend'\n[server]\nidentifier = 'org.xmtp.test'\n[telemetry]\nmetrics_listen = {metrics:?}\n",
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
        "[database]\nurl = {database:?}\n[server]\nidentifier = 'org.xmtp.test'\n[telemetry]\nmetrics_listen = '{}'\n",
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

#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
async fn failed_jwks_startup_exits_before_binding_the_rpc_listener() {
    let database = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://xmtp:xmtp@localhost:55432/xmtp_backend".into());
    let grpc = address();
    let metrics = address();
    let jwks = address();
    let config = format!(
        "[database]\nurl = {database:?}\n[server]\nidentifier = 'org.xmtp.test'\nlisten = {grpc:?}\n[telemetry]\nmetrics_listen = {metrics:?}\n[auth]\nenabled = true\njwks_url = 'http://{jwks}/private-url-sentinel'\n"
    );
    let mut process = Process::start(&config, None);
    assert!(!process.child.wait()?.success());
    let output = process.output();
    assert!(
        output.contains("FetchError") && output.contains("127.0.0.1"),
        "{output}"
    );
    assert!(!output.contains("private-url-sentinel"));
    assert!(!output.contains("backend ready to serve"));
    let _listener = TcpListener::bind(grpc)?;
}

#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
async fn inline_sources_start_and_serve() {
    let database = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://xmtp:xmtp@localhost:55432/xmtp_backend".into());
    for from_env in [false, true] {
        let grpc = address();
        let metrics = address();
        let config = format!(
            "[database]\nurl = {database:?}\n[server]\nidentifier = 'org.xmtp.test'\nlisten = {grpc:?}\n[telemetry]\nmetrics_listen = {metrics:?}\nresource_attributes = {{ secret = 'env:XMTP_STARTUP_TEST_SECRET' }}\n"
        );
        let mut process = if from_env {
            Process::start_with_args("", None, Some(&[]), Some(&config))
        } else {
            Process::start_with_args("", None, Some(&["--config", &config]), None)
        };
        scrape(&metrics, "xmtp_backend_ready 1").await;
        let channel = tonic::transport::Endpoint::from_shared(format!("http://{grpc}"))?
            .connect()
            .await?;
        let mut client = xmtp_backend::api::query_service_client::QueryServiceClient::new(channel);
        client
            .query_newest(xmtp_backend::api::QueryNewestRequest {
                topics: vec![xmtp_backend::api::Topic {
                    topic: xmtp_proto::types::Topic::new_group_message(
                        xmtp_proto::types::GroupId::ZERO,
                    )
                    .cloned_vec(),
                }],
                include_full_envelope: false,
            })
            .await?;
        process.terminate().await;
        assert!(!process.output().contains(SECRET));
    }
}

/// Which push credentials `secret_values` adds.
#[derive(Clone, Copy)]
enum PushSecrets {
    None,
    Apns,
    Fcm,
}

/// Secret values in every secret key. The store URL carries a password and
/// names a database that does not exist, so the connection fails. With push
/// credentials, startup stops at them instead.
fn secret_values(push: PushSecrets) -> String {
    let mut store = url::Url::parse(
        &std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://xmtp:xmtp@localhost:55432/xmtp_backend".into()),
    )
    .expect("DATABASE_URL is a URL");
    store
        .set_password(Some(SECRET))
        .expect("a store URL takes a password");
    store.set_path("xmtp_missing_database");
    let mut config = format!(
        "[server]\nidentifier = 'org.xmtp.test'\nlisten = {grpc:?}\n\
         [telemetry]\nmetrics_listen = {metrics:?}\notlp_endpoint = 'http://127.0.0.1:9/{SECRET}'\n\
         [database]\nurl = '{store}'\nreplica_url = '{store}'\n\
         [chains]\n\"eip155:1\" = 'https://127.0.0.1:9/{SECRET}'\n\
         [attachments]\nbase_url = 'https://files.example.org'\n\
         [attachments.target.S3]\nendpoint = 'https://127.0.0.1:9/{SECRET}'\nregion = 'us-east-1'\nbucket = 'attachments'\n\
         [attachments.target.S3.credentials]\nkind = 'static'\naccess_key_id = '{SECRET}-id'\nsecret_access_key = 'env:XMTP_STARTUP_TEST_SECRET'\nsession_token = '{SECRET}-token'\n",
        grpc = address(),
        metrics = address(),
    );
    match push {
        PushSecrets::None => {}
        PushSecrets::Apns => config.push_str(
            "[push.apns]\nkey = 'env:XMTP_STARTUP_TEST_SECRET'\nkey_id = 'ABC123DEFG'\nteam_id = 'DEF123GHIJ'\nbundle_id = 'org.example.app'\nenvironment = 'production'\n",
        ),
        // Every field but the key is valid, so the sender reaches the key.
        PushSecrets::Fcm => {
            let account = serde_json::json!({
                "type": "service_account",
                "project_id": "example-project",
                "client_email": "push@example-project.iam.gserviceaccount.com",
                "private_key": SECRET,
                "token_uri": "https://oauth2.googleapis.com/token",
            });
            config.push_str(&format!("[push.fcm]\nservice_account = '{account}'\n"));
        }
    }
    config
}

// verifies: CONF-082, CONF-085
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn configuration_source_errors_and_help_omit_secrets() {
    let invalid_value = "[database]\nurl = 'env:XMTP_STARTUP_TEST_SECRET'\n";
    // TOML v1.1.0 lets an inline table span lines. The document parses, so
    // the refusal comes from a check, not from the parser.
    let multiline_table = "[database]\nurl = 'env:XMTP_STARTUP_TEST_SECRET'\n[telemetry]\nresource_attributes = {\n  name = 'value',\n}\n";
    let unreadable = std::env::temp_dir()
        .join(format!("xmtp-missing-{}", uuid::Uuid::new_v4()))
        .join("backend.toml");
    let unreadable = unreadable.to_str()?;
    let secrets = secret_values(PushSecrets::None);
    let apns_secrets = secret_values(PushSecrets::Apns);
    let fcm_secrets = secret_values(PushSecrets::Fcm);
    for (args, env, expected, success) in [
        (vec!["--config-file", unreadable], None, vec!["Read"], false),
        (
            vec!["--config", multiline_table],
            None,
            vec!["Invalid"],
            false,
        ),
        (
            vec!["--config", secrets.as_str()],
            None,
            vec!["Database"],
            false,
        ),
        (
            vec!["--config", apns_secrets.as_str()],
            None,
            vec!["push.apns.key"],
            false,
        ),
        (
            vec!["--config", fcm_secrets.as_str()],
            None,
            vec!["push.fcm.service_account"],
            false,
        ),
        (
            vec!["--config", SECRET, "--config-file", "unused"],
            None,
            vec!["--config", "--config-file"],
            false,
        ),
        (
            vec!["--config-file", "unused"],
            Some(SECRET),
            vec!["--config", "--config-file"],
            false,
        ),
        (
            vec!["--config", "Cargo.toml"],
            None,
            vec!["--config-file"],
            false,
        ),
        (vec![], Some("Cargo.toml"), vec!["--config-file"], false),
        (vec!["--config", "/tmp"], None, vec!["--config-file"], false),
        (vec!["--config", SECRET], None, vec!["Parse"], false),
        (
            vec!["--config", invalid_value],
            None,
            vec!["Invalid"],
            false,
        ),
        (vec![], Some(invalid_value), vec!["Invalid"], false),
        (vec![], Some(SECRET), vec!["Parse"], false),
        (
            vec!["--help"],
            Some(SECRET),
            vec!["XMTP_CONFIG", "--config-file"],
            true,
        ),
        (
            vec!["--config", SECRET, "--help"],
            None,
            vec!["--config-file"],
            true,
        ),
    ] {
        let mut process = Process::start_with_args("", None, Some(&args), env);
        let status = process.child.wait()?;
        let output = process.output();
        assert_eq!(status.success(), success, "{output}");
        assert!(!output.contains(SECRET), "{output}");
        for text in expected {
            assert!(output.contains(text), "{output}");
        }
    }
}

// verifies: CONF-082
#[xmtp_common::test(unwrap_try = true, disable_logging = true)]
fn missing_configuration_source_is_a_clap_usage_error() {
    let mut process = Process::start_with_args("", None, Some(&[]), None);
    assert_eq!(process.child.wait()?.code(), Some(2));
    let output = process.output();
    assert!(
        output.contains("required arguments were not provided"),
        "{output}"
    );
    assert!(output.contains("Usage: xmtp-backend"), "{output}");
    assert!(
        output.contains("--config <CONFIG>|--config-file <CONFIG_FILE>"),
        "{output}"
    );
}
