use std::time::Instant;

use anyhow::{bail, Result};
use chrono::{Duration, Utc};
use clap::Parser;
use edge_core::{
    AlgorithmDsl, AlgorithmInputBinding, AlgorithmKind, AlgorithmOutput, AlgorithmReportMode,
    AlgorithmReportPolicy, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, DataQuality,
    TelemetrySample, TelemetryValue,
};
use edge_runtime::AlgorithmEngine;
use serde::Serialize;

#[derive(Debug, Parser)]
#[command(about = "Run the deterministic Runtime DSL performance gate")]
struct Args {
    #[arg(long, default_value_t = 2_000)]
    iterations: usize,
    #[arg(long, default_value_t = 32)]
    point_count: usize,
    #[arg(long, default_value_t = 10_000.0)]
    min_samples_per_second: f64,
    #[arg(long, default_value_t = 10_000)]
    max_batch_p95_us: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RuntimePerformanceReport {
    status: &'static str,
    iterations: usize,
    point_count: usize,
    algorithm_count: usize,
    input_samples: usize,
    generated_samples: usize,
    duration_ms: u128,
    samples_per_second: f64,
    batch_p95_us: u64,
    minimum_samples_per_second: f64,
    maximum_batch_p95_us: u64,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if args.iterations == 0 {
        bail!("iterations must be greater than zero");
    }
    if args.point_count == 0 || args.point_count > 10_000 {
        bail!("point count must be between 1 and 10000");
    }
    if !args.min_samples_per_second.is_finite() || args.min_samples_per_second <= 0.0 {
        bail!("minimum samples per second must be a positive finite number");
    }
    if args.max_batch_p95_us == 0 {
        bail!("maximum batch P95 must be greater than zero");
    }

    let algorithms = (0..args.point_count)
        .map(change_report_algorithm)
        .collect::<Vec<_>>();
    let mut engine = AlgorithmEngine::new(algorithms)?;
    let started = Instant::now();
    let mut batch_latencies = Vec::with_capacity(args.iterations);
    let mut generated_samples = 0usize;
    let base_time = Utc::now();

    for iteration in 0..args.iterations {
        let timestamp = base_time + Duration::milliseconds(iteration as i64);
        let samples = (0..args.point_count)
            .map(|point| {
                TelemetrySample::new(
                    "performance-device",
                    format!("point-{point}"),
                    TelemetryValue::Float((iteration * args.point_count + point) as f64),
                    DataQuality::Good,
                    timestamp,
                )
            })
            .collect::<Vec<_>>();
        let batch_started = Instant::now();
        let report = engine.apply_samples(&samples)?;
        batch_latencies.push(batch_started.elapsed().as_micros() as u64);
        generated_samples = generated_samples.saturating_add(report.samples.len());
    }

    let elapsed = started.elapsed();
    batch_latencies.sort_unstable();
    let p95_index = ((batch_latencies.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(batch_latencies.len() - 1);
    let batch_p95_us = batch_latencies[p95_index];
    let input_samples = args.iterations.saturating_mul(args.point_count);
    let samples_per_second = input_samples as f64 / elapsed.as_secs_f64().max(f64::EPSILON);
    let passed =
        samples_per_second >= args.min_samples_per_second && batch_p95_us <= args.max_batch_p95_us;
    let report = RuntimePerformanceReport {
        status: if passed { "passed" } else { "failed" },
        iterations: args.iterations,
        point_count: args.point_count,
        algorithm_count: args.point_count,
        input_samples,
        generated_samples,
        duration_ms: elapsed.as_millis(),
        samples_per_second,
        batch_p95_us,
        minimum_samples_per_second: args.min_samples_per_second,
        maximum_batch_p95_us: args.max_batch_p95_us,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if !passed {
        bail!("Runtime DSL performance gate failed");
    }
    Ok(())
}

fn change_report_algorithm(point: usize) -> AlgorithmSpec {
    let source = format!("point-{point}");
    AlgorithmSpec::dsl(
        format!("change-{point}"),
        "1.0.0",
        AlgorithmKind::ChangeReport,
        AlgorithmDsl {
            inputs: vec![AlgorithmInputBinding::new("value", source)],
            trigger: AlgorithmTrigger::on_sample(),
            steps: vec![AlgorithmStep::change_filter("value", 0.0)],
            outputs: vec![AlgorithmOutput::virtual_point(
                "value",
                format!("reported-{point}"),
            )],
            report: AlgorithmReportPolicy::new(AlgorithmReportMode::OnChange, "performance-sink"),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_benchmark_algorithm_has_disconnected_output() {
        let algorithm = change_report_algorithm(7);
        assert_eq!(algorithm.inputs(), vec!["point-7"]);
        assert_eq!(algorithm.dsl.outputs[0].point_id, "reported-7");
    }
}
