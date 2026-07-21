use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use edge_core::{
    AlgorithmKind, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger, CompareOperator, DataQuality,
    EdgeRuntimeEvent, EventSeverity, RuntimeEventCategory, RuntimeEventSeverity, TelemetrySample,
    TelemetryValue, WindowAggregateFunction,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlgorithmExecutionReport {
    pub samples: Vec<TelemetrySample>,
    pub events: Vec<EdgeRuntimeEvent>,
}

pub struct AlgorithmEngine {
    algorithms: Vec<AlgorithmSpec>,
    states: BTreeMap<String, AlgorithmState>,
}

impl AlgorithmEngine {
    pub fn new(algorithms: Vec<AlgorithmSpec>) -> Result<Self> {
        for algorithm in &algorithms {
            if algorithm.id.trim().is_empty() {
                bail!("algorithm id is required");
            }
        }

        Ok(Self {
            algorithms,
            states: BTreeMap::new(),
        })
    }

    pub fn apply_samples(
        &mut self,
        samples: &[TelemetrySample],
    ) -> Result<AlgorithmExecutionReport> {
        let mut report = AlgorithmExecutionReport::default();
        let mut pending = samples.iter().cloned().collect::<VecDeque<_>>();
        let mut processed = BTreeSet::new();
        let max_generated_samples = samples
            .len()
            .max(1)
            .saturating_mul(self.algorithms.len().max(1))
            .saturating_mul(16);

        while let Some(sample) = pending.pop_front() {
            for algorithm in self.algorithms.clone() {
                if algorithm.dsl.inputs.is_empty() {
                    continue;
                }
                if !algorithm.inputs().contains(&sample.telemetry_id) {
                    continue;
                }
                let transition = (
                    algorithm.id.clone(),
                    sample.device_id.clone(),
                    sample.telemetry_id.clone(),
                    sample.timestamp,
                );
                if !processed.insert(transition) {
                    continue;
                }
                let state = self.states.entry(algorithm.id.clone()).or_default();
                let mut generated = match algorithm.kind {
                    AlgorithmKind::ChangeReport => apply_change_report(&algorithm, state, &sample)?,
                    AlgorithmKind::WindowAggregate => {
                        apply_window_aggregate(&algorithm, state, &sample)?
                    }
                    AlgorithmKind::ExpressionAggregate => {
                        apply_expression(&algorithm, state, &sample)?
                    }
                    AlgorithmKind::ThresholdRule => {
                        report
                            .events
                            .extend(apply_threshold_rule(&algorithm, state, &sample)?);
                        Vec::new()
                    }
                    AlgorithmKind::DurationRule
                    | AlgorithmKind::Deadband
                    | AlgorithmKind::Debounce
                    | AlgorithmKind::Statistics => Vec::new(),
                };
                if report.samples.len().saturating_add(generated.len()) > max_generated_samples {
                    bail!("algorithm graph generated too many samples; check for a cycle");
                }
                pending.extend(generated.iter().cloned());
                report.samples.append(&mut generated);
            }
        }
        Ok(report)
    }
}

#[derive(Clone, Debug, Default)]
struct AlgorithmState {
    last_reported: BTreeMap<String, TelemetryValue>,
    latest_by_alias: BTreeMap<String, TelemetrySample>,
    windows: BTreeMap<String, WindowState>,
}

#[derive(Clone, Debug, Default)]
struct WindowState {
    started_at: Option<DateTime<Utc>>,
    samples: Vec<TelemetrySample>,
}

fn apply_change_report(
    algorithm: &AlgorithmSpec,
    state: &mut AlgorithmState,
    sample: &TelemetrySample,
) -> Result<Vec<TelemetrySample>> {
    let Some(step) = algorithm.dsl.steps.iter().find_map(|step| match step {
        AlgorithmStep::ChangeFilter { source, threshold } => Some((source, *threshold)),
        _ => None,
    }) else {
        return Ok(Vec::new());
    };
    let (source, threshold) = step;
    let Some(binding) = algorithm
        .dsl
        .inputs
        .iter()
        .find(|input| input.alias == *source)
    else {
        bail!(
            "algorithm {} references missing input alias {}",
            algorithm.id,
            source
        );
    };
    if binding.point_id != sample.telemetry_id {
        return Ok(Vec::new());
    }
    let Some(output) = algorithm.dsl.outputs.first() else {
        bail!("algorithm {} requires an output", algorithm.id);
    };
    let key = output.point_id.clone();
    let should_emit = state
        .last_reported
        .get(&key)
        .map(|last| value_changed(last, &sample.value, threshold))
        .unwrap_or(true);
    if !should_emit {
        return Ok(Vec::new());
    }
    state.last_reported.insert(key, sample.value.clone());
    Ok(vec![TelemetrySample::new(
        sample.device_id.clone(),
        output.point_id.clone(),
        sample.value.clone(),
        sample.quality,
        sample.timestamp,
    )])
}

fn apply_window_aggregate(
    algorithm: &AlgorithmSpec,
    state: &mut AlgorithmState,
    sample: &TelemetrySample,
) -> Result<Vec<TelemetrySample>> {
    let every_ms = match algorithm.dsl.trigger {
        AlgorithmTrigger::Window { every_ms } => every_ms,
        _ => return Ok(Vec::new()),
    };
    let Some((source, functions)) = algorithm.dsl.steps.iter().find_map(|step| match step {
        AlgorithmStep::WindowAggregate { source, functions } => Some((source, functions)),
        _ => None,
    }) else {
        return Ok(Vec::new());
    };
    let Some(binding) = algorithm
        .dsl
        .inputs
        .iter()
        .find(|input| input.alias == *source)
    else {
        bail!(
            "algorithm {} references missing input alias {}",
            algorithm.id,
            source
        );
    };
    if binding.point_id != sample.telemetry_id {
        return Ok(Vec::new());
    }

    let window = state.windows.entry(source.clone()).or_default();
    let started_at = window.started_at.get_or_insert(sample.timestamp);
    window.samples.push(sample.clone());
    if (sample.timestamp - *started_at).num_milliseconds() < every_ms as i64 {
        return Ok(Vec::new());
    }

    let values = window
        .samples
        .iter()
        .filter_map(|sample| sample.value.as_f64())
        .collect::<Vec<_>>();
    let mut outputs = Vec::new();
    for function in functions {
        if let Some((name, value)) = aggregate_value(function, &values) {
            if let Some(output) = algorithm
                .dsl
                .outputs
                .iter()
                .find(|output| output.name == name)
            {
                outputs.push(TelemetrySample::new(
                    sample.device_id.clone(),
                    output.point_id.clone(),
                    TelemetryValue::Float(value),
                    DataQuality::Good,
                    sample.timestamp,
                ));
            }
        }
    }
    window.started_at = Some(sample.timestamp);
    window.samples.clear();
    Ok(outputs)
}

fn apply_expression(
    algorithm: &AlgorithmSpec,
    state: &mut AlgorithmState,
    sample: &TelemetrySample,
) -> Result<Vec<TelemetrySample>> {
    for input in &algorithm.dsl.inputs {
        if input.point_id == sample.telemetry_id {
            state
                .latest_by_alias
                .insert(input.alias.clone(), sample.clone());
        }
    }
    let Some((output_name, expr)) = algorithm.dsl.steps.iter().find_map(|step| match step {
        AlgorithmStep::Expression { output, expr } => Some((output, expr)),
        _ => None,
    }) else {
        return Ok(Vec::new());
    };
    if !algorithm
        .dsl
        .inputs
        .iter()
        .all(|input| state.latest_by_alias.contains_key(&input.alias))
    {
        return Ok(Vec::new());
    }
    let value = evaluate_addition_expression(expr, &state.latest_by_alias)?;
    let Some(output) = algorithm
        .dsl
        .outputs
        .iter()
        .find(|output| output.name == *output_name)
    else {
        bail!(
            "algorithm {} references missing output {}",
            algorithm.id,
            output_name
        );
    };
    Ok(vec![TelemetrySample::new(
        sample.device_id.clone(),
        output.point_id.clone(),
        TelemetryValue::Float(value),
        DataQuality::Good,
        sample.timestamp,
    )])
}

fn apply_threshold_rule(
    algorithm: &AlgorithmSpec,
    _state: &mut AlgorithmState,
    sample: &TelemetrySample,
) -> Result<Vec<EdgeRuntimeEvent>> {
    let mut events = Vec::new();
    for step in &algorithm.dsl.steps {
        let AlgorithmStep::ThresholdRule {
            source,
            operator,
            threshold,
            event,
        } = step
        else {
            continue;
        };
        let Some(binding) = algorithm
            .dsl
            .inputs
            .iter()
            .find(|input| input.alias == *source)
        else {
            bail!(
                "algorithm {} references missing input alias {}",
                algorithm.id,
                source
            );
        };
        if binding.point_id != sample.telemetry_id {
            continue;
        }
        let Some(value) = sample.value.as_f64() else {
            continue;
        };
        if compare(value, *operator, *threshold) {
            events.push(
                EdgeRuntimeEvent::new(
                    "",
                    map_event_severity(event.severity),
                    RuntimeEventCategory::Algorithm,
                    event.code.clone(),
                    event.message.clone(),
                )
                .with_context("algorithm_id", algorithm.id.clone())
                .with_context("point_id", sample.telemetry_id.clone()),
            );
        }
    }
    Ok(events)
}

fn value_changed(previous: &TelemetryValue, current: &TelemetryValue, threshold: f64) -> bool {
    match (previous.as_f64(), current.as_f64()) {
        (Some(previous), Some(current)) => (current - previous).abs() >= threshold,
        _ => previous != current,
    }
}

fn aggregate_value(function: &WindowAggregateFunction, values: &[f64]) -> Option<(String, f64)> {
    match function {
        WindowAggregateFunction::Avg { output } => non_empty(values).map(|values| {
            (
                output.clone(),
                values.iter().sum::<f64>() / values.len() as f64,
            )
        }),
        WindowAggregateFunction::Min { output } => non_empty(values).map(|values| {
            (
                output.clone(),
                values.iter().copied().fold(f64::INFINITY, f64::min),
            )
        }),
        WindowAggregateFunction::Max { output } => non_empty(values).map(|values| {
            (
                output.clone(),
                values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            )
        }),
        WindowAggregateFunction::Sum { output } => {
            Some((output.clone(), values.iter().sum::<f64>()))
        }
        WindowAggregateFunction::Count { output } => Some((output.clone(), values.len() as f64)),
        WindowAggregateFunction::First { output } => {
            values.first().copied().map(|value| (output.clone(), value))
        }
        WindowAggregateFunction::Last { output } => {
            values.last().copied().map(|value| (output.clone(), value))
        }
    }
}

fn non_empty(values: &[f64]) -> Option<&[f64]> {
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn evaluate_addition_expression(
    expr: &str,
    values: &BTreeMap<String, TelemetrySample>,
) -> Result<f64> {
    let mut total = 0.0;
    for term in expr.split('+') {
        let alias = term.trim();
        let Some(value) = values.get(alias).and_then(|sample| sample.value.as_f64()) else {
            bail!("expression references missing numeric alias `{alias}`");
        };
        total += value;
    }
    Ok(total)
}

fn compare(value: f64, operator: CompareOperator, threshold: f64) -> bool {
    match operator {
        CompareOperator::Gt => value > threshold,
        CompareOperator::Gte => value >= threshold,
        CompareOperator::Lt => value < threshold,
        CompareOperator::Lte => value <= threshold,
        CompareOperator::Eq => (value - threshold).abs() < f64::EPSILON,
        CompareOperator::Ne => (value - threshold).abs() >= f64::EPSILON,
    }
}

fn map_event_severity(severity: EventSeverity) -> RuntimeEventSeverity {
    match severity {
        EventSeverity::Info => RuntimeEventSeverity::Info,
        EventSeverity::Warning => RuntimeEventSeverity::Warning,
        EventSeverity::Critical => RuntimeEventSeverity::Critical,
    }
}
