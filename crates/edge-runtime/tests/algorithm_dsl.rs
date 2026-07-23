use chrono::{Duration, Utc};
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CompareOperator,
    DataQuality, TelemetrySample, TelemetryValue, WindowAggregateFunction,
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

#[test]
fn algorithm_outputs_flow_into_downstream_compute_nodes() {
    let upstream = AlgorithmSpec::dsl(
        "pressure-change",
        "v1",
        AlgorithmKind::ChangeReport,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::change_filter("p", 0.0)],
            outputs: vec![AlgorithmOutput::virtual_point("value", "pressure.reported")],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
        },
    );
    let downstream = AlgorithmSpec::dsl(
        "pressure-forward",
        "v1",
        AlgorithmKind::ChangeReport,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure.reported")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::change_filter("p", 0.0)],
            outputs: vec![AlgorithmOutput::virtual_point(
                "value",
                "pressure.forwarded",
            )],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "velamq-main"),
        },
    );
    let mut engine = AlgorithmEngine::new(vec![upstream, downstream]).expect("engine builds");

    let report = engine
        .apply_samples(&[sample("pressure", 10.0, Utc::now())])
        .unwrap();

    assert_eq!(report.samples.len(), 2);
    assert_eq!(report.samples[0].telemetry_id, "pressure.reported");
    assert_eq!(report.samples[1].telemetry_id, "pressure.forwarded");
}

#[test]
fn common_transform_nodes_scale_clamp_and_calculate_rate() {
    let algorithms = vec![
        transform_algorithm(
            "scale",
            "pressure.scaled",
            AlgorithmStep::Scale {
                source: "p".to_string(),
                output: "value".to_string(),
                factor: 2.0,
                offset: 1.0,
            },
        ),
        transform_algorithm(
            "clamp",
            "pressure.clamped",
            AlgorithmStep::Clamp {
                source: "p".to_string(),
                output: "value".to_string(),
                min: 0.0,
                max: 10.0,
            },
        ),
        transform_algorithm(
            "rate",
            "pressure.rate",
            AlgorithmStep::RateOfChange {
                source: "p".to_string(),
                output: "value".to_string(),
                per_ms: 1_000,
            },
        ),
    ];
    let mut engine = AlgorithmEngine::new(algorithms).unwrap();
    let t0 = Utc::now();

    let first = engine
        .apply_samples(&[sample("pressure", 7.0, t0)])
        .unwrap();
    let second = engine
        .apply_samples(&[sample("pressure", 13.0, t0 + Duration::seconds(2))])
        .unwrap();

    assert_eq!(sample_value(&first.samples, "pressure.scaled"), Some(15.0));
    assert_eq!(sample_value(&first.samples, "pressure.clamped"), Some(7.0));
    assert_eq!(sample_value(&second.samples, "pressure.scaled"), Some(27.0));
    assert_eq!(
        sample_value(&second.samples, "pressure.clamped"),
        Some(10.0)
    );
    assert_eq!(sample_value(&second.samples, "pressure.rate"), Some(3.0));
}

#[test]
fn expression_supports_precedence_parentheses_and_common_functions() {
    let algorithm = AlgorithmSpec::dsl(
        "formula",
        "v1",
        AlgorithmKind::ExpressionAggregate,
        AlgorithmDsl {
            inputs: vec![
                AlgorithmInputBinding::new("p0", "pressure"),
                AlgorithmInputBinding::new("p1", "temperature"),
            ],
            trigger: AlgorithmTrigger::on_any_input(),
            steps: vec![AlgorithmStep::Expression {
                output: "value".to_string(),
                expr: "round(max(p0 * 2, p1) / 3)".to_string(),
            }],
            outputs: vec![AlgorithmOutput::virtual_point("value", "formula.output")],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnOutput, "velamq-main"),
        },
    );
    let mut engine = AlgorithmEngine::new(vec![algorithm]).unwrap();
    let t0 = Utc::now();

    engine
        .apply_samples(&[sample("pressure", 10.0, t0)])
        .unwrap();
    let output = engine
        .apply_samples(&[sample("temperature", 13.0, t0)])
        .unwrap();

    assert_eq!(sample_value(&output.samples, "formula.output"), Some(7.0));
}

#[test]
fn debounce_waits_until_value_is_stable() {
    let algorithm = AlgorithmSpec::dsl(
        "running-debounce",
        "v1",
        AlgorithmKind::Debounce,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("running", "running")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::Debounce {
                source: "running".to_string(),
                stable_ms: 1_000,
            }],
            outputs: vec![AlgorithmOutput::virtual_point("value", "running.stable")],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnOutput, "velamq-main"),
        },
    );
    let mut engine = AlgorithmEngine::new(vec![algorithm]).unwrap();
    let t0 = Utc::now();
    let boolean_sample = |value, timestamp| {
        TelemetrySample::new(
            "pump-1",
            "running",
            TelemetryValue::Boolean(value),
            DataQuality::Good,
            timestamp,
        )
    };

    assert!(engine
        .apply_samples(&[boolean_sample(true, t0)])
        .unwrap()
        .samples
        .is_empty());
    assert!(engine
        .apply_samples(&[boolean_sample(false, t0 + Duration::milliseconds(500))])
        .unwrap()
        .samples
        .is_empty());
    let stable = engine
        .apply_samples(&[boolean_sample(false, t0 + Duration::milliseconds(1_500))])
        .unwrap();

    assert_eq!(stable.samples[0].telemetry_id, "running.stable");
    assert_eq!(stable.samples[0].value, TelemetryValue::Boolean(false));
}

#[test]
fn conditional_route_emits_one_named_branch_that_can_fan_out() {
    let route = AlgorithmSpec::dsl(
        "pressure-route",
        "v1",
        AlgorithmKind::ThresholdRule,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::ConditionalRoute {
                source: "p".to_string(),
                operator: CompareOperator::Gte,
                threshold: 10.0,
                matched_output: "matched".to_string(),
                unmatched_output: "unmatched".to_string(),
            }],
            outputs: vec![
                AlgorithmOutput::virtual_point("matched", "pressure.matched"),
                AlgorithmOutput::virtual_point("unmatched", "pressure.unmatched"),
            ],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnOutput, "velamq-main"),
        },
    );
    let downstream = |id: &str, output: &str| {
        AlgorithmSpec::dsl(
            id,
            "v1",
            AlgorithmKind::ChangeReport,
            AlgorithmDsl {
                inputs: vec![AlgorithmInputBinding::new("p", "pressure.matched")],
                trigger: AlgorithmTrigger::on_sample(),
                steps: vec![AlgorithmStep::change_filter("p", 0.0)],
                outputs: vec![AlgorithmOutput::virtual_point("value", output)],
                report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnOutput, "velamq-main"),
            },
        )
    };
    let mut engine = AlgorithmEngine::new(vec![
        route,
        downstream("branch-a", "branch.a"),
        downstream("branch-b", "branch.b"),
    ])
    .unwrap();

    let matched = engine
        .apply_samples(&[sample("pressure", 12.0, Utc::now())])
        .unwrap();

    assert_eq!(
        sample_value(&matched.samples, "pressure.matched"),
        Some(12.0)
    );
    assert_eq!(sample_value(&matched.samples, "pressure.unmatched"), None);
    assert_eq!(sample_value(&matched.samples, "branch.a"), Some(12.0));
    assert_eq!(sample_value(&matched.samples, "branch.b"), Some(12.0));
}

fn transform_algorithm(id: &str, output: &str, step: AlgorithmStep) -> AlgorithmSpec {
    AlgorithmSpec::dsl(
        id,
        "v1",
        AlgorithmKind::ExpressionAggregate,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("p", "pressure")],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![step],
            outputs: vec![AlgorithmOutput::virtual_point("value", output)],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnOutput, "velamq-main"),
        },
    )
}

fn sample_value(samples: &[TelemetrySample], point_id: &str) -> Option<f64> {
    samples
        .iter()
        .find(|sample| sample.telemetry_id == point_id)
        .and_then(|sample| sample.value.as_f64())
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
