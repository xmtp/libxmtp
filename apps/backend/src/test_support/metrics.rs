//! Process isolation for tests that observe metrics from spawned server tasks.
use metrics_exporter_prometheus::PrometheusHandle;

const CHILD_TEST: &str = "XMTP_BACKEND_METRICS_TEST";

/// Run this existing test alone before installing its process-wide recorder.
/// Other tests continue in parallel without contributing to its snapshot.
fn child(name: &str) -> bool {
    if std::env::var(CHILD_TEST).as_deref() != Ok(name) {
        let output = std::process::Command::new(std::env::current_exe().expect("test executable"))
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(CHILD_TEST, name)
            .output()
            .expect("run isolated metric test");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && stdout.contains("1 passed"),
            "isolated test {name} failed: {stdout}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return false;
    }
    true
}

pub(crate) fn isolated(name: &str) -> Option<PrometheusHandle> {
    if !child(name) {
        return None;
    }
    let handle = crate::telemetry::recorder_builder()
        .expect("metric buckets")
        .install_recorder()
        .expect("isolated recorder");
    crate::telemetry::describe();
    Some(handle)
}

/// Read one metric family with the requested fixed labels. Missing series are zero.
pub(crate) fn value(handle: &PrometheusHandle, name: &str, labels: &[(&str, &str)]) -> f64 {
    handle
        .render()
        .lines()
        .filter(|line| {
            (line.starts_with(&format!("{name}{{")) || line.starts_with(&format!("{name} ")))
                && labels
                    .iter()
                    .all(|(key, value)| line.contains(&format!("{key}=\"{value}\"")))
        })
        .map(|line| {
            line.split_whitespace()
                .last()
                .expect("metric value")
                .parse::<f64>()
                .expect("numeric metric")
        })
        .sum()
}

/// Bind the real exporter for tests that must inspect its HTTP representation.
pub(crate) fn isolated_http(name: &str) -> Option<(PrometheusHandle, std::net::SocketAddr)> {
    if !child(name) {
        return None;
    }
    let reservation = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve metric address");
    let address = reservation.local_addr().expect("metric address");
    drop(reservation);
    let (recorder, exporter) = crate::telemetry::recorder_builder()
        .expect("metric buckets")
        .with_http_listener(address)
        .build()
        .expect("metric listener");
    let handle = recorder.handle();
    metrics::set_global_recorder(recorder).expect("isolated recorder");
    tokio::spawn(exporter);
    crate::telemetry::describe();
    Some((handle, address))
}

pub(crate) struct LogDirectory(pub std::path::PathBuf);
impl LogDirectory {
    pub(crate) fn new() -> Self {
        let directory =
            std::env::temp_dir().join(format!("xmtp-telemetry-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).expect("test log directory");
        Self(directory)
    }
    pub(crate) fn read(&self) -> String {
        std::fs::read_dir(&self.0)
            .expect("test log files")
            .map(|entry| {
                std::fs::read_to_string(entry.expect("test log file").path())
                    .expect("test log text")
            })
            .collect()
    }
}
impl Drop for LogDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
