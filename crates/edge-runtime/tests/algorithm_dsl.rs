use chrono::{Duration, Utc};
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, DataQuality,
    TelemetrySample, TelemetryValue, WindowAggregateFunction,
};
use edge_runtime::AlgorithmEngine;

#[test]
fn change_report_emits_only_samples_that_exceed_threshold() {
    let algorithm = AlgorithmSpec::dsl(
        "pressure-change",
        "v1",
        AlgorithmKind::ChangeReport,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::change_filter("p", 0.5)],
            outputs: vec![AlgorithmOutput::virtual_point("p", "pressure.reported")],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
        },
    );
    let mut engine = AlgorithmEngine::new(vec![algorithm]).expect("engine builds");
    let t0 = Utc::now();

    let first = engine
        .apply_samples(&[sample("pressure", 10.0, t0)])
        .unwrap();
    let small_change = engine
        .apply_samples(&[sample("pressure", 10.2, t0 + Duration::seconds(1))])
        .unwrap();
    let large_change = engine
        .apply_samples(&[sample("pressure", 10.8, t0 + Duration::seconds(2))])
        .unwrap();

    assert_eq!(first.samples.len(), 1);
    assert_eq!(small_change.samples.len(), 0);
    assert_eq!(large_change.samples.len(), 1);
    assert_eq!(large_change.samples[0].telemetry_id, "pressure.reported");
    assert_eq!(large_change.samples[0].value, TelemetryValue::Float(10.8));
}

#[test]
fn window_aggregate_emits_virtual_point_when_window_closes() {
    let algorithm = AlgorithmSpec::dsl(
        "pressure-avg",
        "v1",
        AlgorithmKind::WindowAggregate,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::window(60_000),
            steps: vec![AlgorithmStep::window_aggregate(
                "p",
                vec![WindowAggregateFunction::Avg {
                    output: "pressure_avg".to_string(),
                }],
            )],
            outputs: vec![AlgorithmOutput::virtual_point(
                "pressure_avg",
                "pressure.avg_1m",
            )],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::WindowResult, "velamq-main"),
        },
    );
    let mut engine = AlgorithmEngine::new(vec![algorithm]).expect("engine builds");
    let t0 = Utc::now();

    assert_eq!(
        engine
            .apply_samples(&[sample("pressure", 10.0, t0)])
            .unwrap()
            .samples
            .len(),
        0
    );
    let output = engine
        .apply_samples(&[sample("pressure", 14.0, t0 + Duration::seconds(60))])
        .unwrap();

    assert_eq!(output.samples.len(), 1);
    assert_eq!(output.samples[0].telemetry_id, "pressure.avg_1m");
    assert_eq!(output.samples[0].value, TelemetryValue::Float(12.0));
}

fn sample(point_id: &str, value: f64, timestamp: chrono::DateTime<Utc>) -> TelemetrySample {
    TelemetrySample::new(
        "pump-1",
        point_id,
        TelemetryValue::Float(value),
        DataQuality::Good,
        timestamp,
    )
}
