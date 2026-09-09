use super::*;

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
fn outbound_wait_omits_zero_duration() {
    let recorder = recorder_builder()?.build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        stream_outbound_waited(Duration::ZERO);
    });
    assert!(
        !handle
            .render()
            .contains("xmtp_stream_outbound_wait_seconds")
    );
}
