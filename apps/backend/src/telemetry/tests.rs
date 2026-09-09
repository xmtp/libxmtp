use super::*;
use crate::test_support::{self as support, metrics::value};

#[xmtp_common::test(unwrap_try = true)]
fn catalogue_matches_documented_types_and_help_for_every_metric() {
    let document = include_str!("../../../../docs/specs/002_backend_architecture.md");
    let table = document
        .split("### Backend metric catalogue")
        .nth(1)?
        .split("## 8.")
        .next()?;
    let names: Vec<_> = table
        .lines()
        .filter(|line| line.starts_with("| `"))
        .collect();
    assert_eq!(names.len(), CATALOGUE.len());
    let guide = include_str!("../../../../docs/backend-observability.md");
    let guide_rows: Vec<_> = guide
        .split("## Metric catalogue")
        .nth(1)?
        .split("## Span names")
        .next()?
        .lines()
        .filter(|line| line.starts_with("| `"))
        .collect();
    assert_eq!(guide_rows.len(), CATALOGUE.len());
    let recorder = recorder_builder()?.build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, describe);
    use metrics::Recorder;
    let metadata =
        metrics::Metadata::new(module_path!(), metrics::Level::INFO, Some(module_path!()));
    for spec in CATALOGUE {
        let kind = match spec.kind {
            MetricType::Counter => "counter",
            MetricType::Gauge => "gauge",
            MetricType::Histogram => "histogram",
        };
        assert!(
            names.contains(&format!("| `{}` | {kind} | {} |", spec.name, spec.help).as_str()),
            "{}",
            spec.name
        );
        let prefix = format!("| `{}` | {kind} | {} |", spec.name, spec.help);
        assert_eq!(
            guide_rows
                .iter()
                .filter(|row| row.starts_with(&prefix))
                .count(),
            1,
            "{} must appear once in the observability guide",
            spec.name
        );
        let key = metrics::Key::from_static_name(spec.name);
        match spec.kind {
            MetricType::Counter => recorder.register_counter(&key, &metadata).increment(1),
            MetricType::Gauge => recorder.register_gauge(&key, &metadata).set(1.0),
            MetricType::Histogram => recorder.register_histogram(&key, &metadata).record(1.0),
        }
    }
    let output = handle.render();
    for spec in CATALOGUE {
        assert!(output.contains(&format!("# HELP {} {}", spec.name, spec.help)));
        assert!(output.contains(&format!("# TYPE {} ", spec.name)));
    }
}

#[xmtp_common::test(unwrap_try = true)]
fn live_lag_clamps_clock_skew_and_uses_wider_buckets() {
    const ADMITTED_NS: i64 = 10 * xmtp_common::NS_IN_SEC;
    let recorder = recorder_builder()?.build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        stream_live_admitted(ADMITTED_NS, (ADMITTED_NS - xmtp_common::NS_IN_SEC) as u64);
        stream_live_admitted(ADMITTED_NS, (ADMITTED_NS + xmtp_common::NS_IN_SEC) as u64);
        stream_outbound_waited(Duration::ZERO);
    });
    assert_eq!(
        value(&handle, "xmtp_stream_delivery_lag_seconds_count", &[]),
        2.0
    );
    assert_eq!(
        value(&handle, "xmtp_stream_delivery_lag_seconds_sum", &[]),
        1.0
    );
    assert!(handle.render().contains("le=\"60\""));
    assert!(
        !handle
            .render()
            .contains("xmtp_stream_outbound_wait_seconds")
    );
}

#[xmtp_common::test(unwrap_try = true)]
async fn primary_only_sampler_keeps_last_sequence_on_timeout_and_recovers_next_tick() {
    let Some(metrics) = support::metrics::isolated(
        "telemetry::tests::primary_only_sampler_keeps_last_sequence_on_timeout_and_recovers_next_tick",
    ) else {
        return;
    };
    let server =
        support::TestServer::new(|config| config.database.max_statement_timeout_ms = 100).await?;
    xmtp_common::wait_for_some(|| async {
        metrics
            .render()
            .contains("xmtp_sequence_id{database=\"primary\"}")
            .then_some(())
    })
    .await?;
    assert_eq!(
        value(&metrics, "xmtp_sequence_id", &[("database", "primary")]),
        0.0
    );
    server
        .publish(vec![
            xmtp_mls_validation::test_utils::inline_welcome_envelope([90; 32]),
        ])
        .await?;
    xmtp_common::wait_for_eq(
        || async { value(&metrics, "xmtp_sequence_id", &[("database", "primary")]) },
        1.0,
    )
    .await?;
    server
        .publish(vec![
            xmtp_mls_validation::test_utils::inline_welcome_envelope([91; 32]),
        ])
        .await?;
    let mut blocker = server.backend.store.primary.begin().await?;
    sqlx::query("LOCK TABLE envelopes IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await?;
    xmtp_common::wait_for_ge(
        || async {
            value(
                &metrics,
                "xmtp_telemetry_sampler_errors_total",
                &[("sample", "primary_sequence")],
            )
        },
        1.0,
    )
    .await?;
    assert_eq!(
        value(&metrics, "xmtp_sequence_id", &[("database", "primary")]),
        1.0
    );
    blocker.rollback().await?;
    xmtp_common::wait_for_eq(
        || async { value(&metrics, "xmtp_sequence_id", &[("database", "primary")]) },
        2.0,
    )
    .await?;
    let output = metrics.render();
    assert!(!output.contains("pool=\"read\""));
    assert!(!output.contains("database=\"read\""));
    assert!(output.contains("xmtp_db_pool_connections{pool=\"primary\",state=\"idle\"}"));
    for name in [
        "tokio_runtime_workers",
        "tokio_runtime_alive_tasks",
        "tokio_runtime_global_queue_depth",
        "tokio_runtime_worker_busy_seconds_total",
        "xmtp_stream_fetch_workers_in_use",
    ] {
        assert!(output.lines().any(|line| line.starts_with(name)), "{name}");
    }
    server.stop().await?;
}
