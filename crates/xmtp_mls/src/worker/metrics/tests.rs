use super::*;
use tracing::instrument::WithSubscriber;
use xmtp_common::TestWriter;

fn capture() -> (TestWriter, impl tracing::Subscriber + Send + Sync) {
    let writer = TestWriter::new();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_writer(writer.clone())
        .with_ansi(false)
        .without_time()
        .finish();
    (writer, subscriber)
}

#[xmtp_common::test(unwrap_try = true)]
fn metric_trace_omits_installation_id() {
    let installation_id: InstallationId = [0x7a; 32].into();
    let metrics = WorkerMetrics::new(installation_id);
    let (writer, subscriber) = capture();
    tracing::subscriber::with_default(subscriber, || metrics.increment_metric("completed"));
    assert_eq!(metrics.get("completed"), 1);
    let output = writer.as_string();
    assert!(output.contains("firing \"completed\""));
    assert!(!output.contains(&hex::encode(installation_id)));
}

#[xmtp_common::test(unwrap_try = true)]
async fn metric_wait_log_omits_installation_id() {
    let installation_id: InstallationId = [0x7a; 32].into();
    let metrics = WorkerMetrics::new(installation_id);
    metrics.increment_metric("completed");
    let interest = metrics.register_interest("completed", 1);
    let (writer, subscriber) = capture();
    interest.wait().with_subscriber(subscriber).await?;
    let output = writer.as_string();
    assert!(output.contains("successfully waited for \"completed\""));
    assert!(!output.contains(&hex::encode(installation_id)));
}
