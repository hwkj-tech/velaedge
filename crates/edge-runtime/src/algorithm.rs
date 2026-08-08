use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::{Duration, Instant},
};

use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use edge_core::{
    AlgorithmKind, AlgorithmRuntimeMetrics, AlgorithmSpec, AlgorithmStep, AlgorithmTrigger,
    CompareOperator, DataQuality, EdgeRuntimeEvent, EventSeverity, RuntimeEventCategory,
    RuntimeEventSeverity, TelemetrySample, TelemetryValue, WindowAggregateFunction,
};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlgorithmExecutionReport {
    pub samples: Vec<TelemetrySample>,
    pub events: Vec<EdgeRuntimeEvent>,
}

pub struct AlgorithmEngine {
    algorithms: Vec<AlgorithmSpec>,
    states: BTreeMap<String, AlgorithmState>,
    execution: BTreeMap<String, AlgorithmExecutionStats>,
}

#[derive(Clone, Debug, Default)]
struct AlgorithmExecutionStats {
    healthy: bool,
    last_run_latency_ms: u64,
    error_count: u64,
    alert_count: u64,
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
            execution: BTreeMap::new(),
        })
    }

    pub fn runtime_metrics(&self) -> Vec<AlgorithmRuntimeMetrics> {
        self.algorithms
            .iter()
            .filter_map(|algorithm| {
                self.execution
                    .get(&algorithm.id)
                    .map(|stats| AlgorithmRuntimeMetrics {
                        algorithm_id: algorithm.id.clone(),
                        healthy: stats.healthy,
                        last_run_latency_ms: stats.last_run_latency_ms,
                        error_count: stats.error_count,
                        alert_count: stats.alert_count,
                    })
            })
            .collect()
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
                let started = Instant::now();
                let execution = match algorithm.kind {
                    AlgorithmKind::ChangeReport | AlgorithmKind::Deadband => {
                        apply_change_report(&algorithm, state, &sample)
                            .map(|samples| (samples, Vec::new()))
                    }
                    AlgorithmKind::WindowAggregate | AlgorithmKind::Statistics => {
                        apply_window_aggregate(&algorithm, state, &sample)
                            .map(|samples| (samples, Vec::new()))
                    }
                    AlgorithmKind::ExpressionAggregate => {
                        apply_transform(&algorithm, state, &sample)
                            .map(|samples| (samples, Vec::new()))
                    }
                    AlgorithmKind::ThresholdRule => {
                        apply_threshold_rule(&algorithm, state, &sample)
                    }
                    AlgorithmKind::Debounce => apply_debounce(&algorithm, state, &sample)
                        .map(|samples| (samples, Vec::new())),
                    AlgorithmKind::DurationRule => {
                        apply_duration_condition(&algorithm, state, &sample)
                            .map(|samples| (samples, Vec::new()))
                    }
                };
                let stats = self.execution.entry(algorithm.id.clone()).or_default();
                stats.last_run_latency_ms = elapsed_millis(started.elapsed());
                let (mut generated, events) = match execution {
                    Ok(result) => {
                        stats.healthy = true;
                        result
                    }
                    Err(error) => {
                        stats.healthy = false;
                        stats.error_count = stats.error_count.saturating_add(1);
                        return Err(error);
                    }
                };
                stats.alert_count = stats.alert_count.saturating_add(events.len() as u64);
                report.events.extend(events);
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

fn elapsed_millis(duration: Duration) -> u64 {
    if duration.is_zero() {
        0
    } else {
        duration.as_millis().max(1).min(u128::from(u64::MAX)) as u64
    }
}

#[derive(Clone, Debug, Default)]
struct AlgorithmState {
    last_reported: BTreeMap<String, TelemetryValue>,
    latest_by_alias: BTreeMap<String, TelemetrySample>,
    previous_by_alias: BTreeMap<String, TelemetrySample>,
    debounce_by_alias: BTreeMap<String, DebounceState>,
    duration_by_alias: BTreeMap<String, DurationConditionState>,
    windows: BTreeMap<String, WindowState>,
}

#[derive(Clone, Debug)]
struct DebounceState {
    value: TelemetryValue,
    since: DateTime<Utc>,
}

#[derive(Clone, Debug)]
struct DurationConditionState {
    since: DateTime<Utc>,
    emitted: bool,
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
    )
    .inherit_quality(sample)])
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
    let quality_source = window
        .samples
        .iter()
        .max_by_key(|window_sample| data_quality_rank(window_sample.quality))
        .unwrap_or(sample)
        .clone();
    let mut outputs = Vec::new();
    for function in functions {
        if let Some((name, value)) = aggregate_value(function, &values) {
            if let Some(output) = algorithm
                .dsl
                .outputs
                .iter()
                .find(|output| output.name == name)
            {
                outputs.push(
                    TelemetrySample::new(
                        sample.device_id.clone(),
                        output.point_id.clone(),
                        value,
                        quality_source.quality,
                        sample.timestamp,
                    )
                    .inherit_quality(&quality_source),
                );
            }
        }
    }
    window.started_at = Some(sample.timestamp);
    window.samples.clear();
    Ok(outputs)
}

fn apply_transform(
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
    let Some(result) = algorithm.dsl.steps.iter().find_map(|step| match step {
        AlgorithmStep::Expression { output, expr } => {
            if !algorithm
                .dsl
                .inputs
                .iter()
                .all(|input| state.latest_by_alias.contains_key(&input.alias))
            {
                return None;
            }
            Some(evaluate_expression(expr, &state.latest_by_alias).map(|value| (output, value)))
        }
        AlgorithmStep::Scale {
            source,
            output,
            factor,
            offset,
        } => numeric_source(algorithm, state, sample, source)
            .map(|value| Ok((output, value * factor + offset))),
        AlgorithmStep::Clamp {
            source,
            output,
            min,
            max,
        } => numeric_source(algorithm, state, sample, source)
            .map(|value| Ok((output, value.clamp(*min, *max)))),
        AlgorithmStep::RateOfChange {
            source,
            output,
            per_ms,
        } => {
            let binding = algorithm
                .dsl
                .inputs
                .iter()
                .find(|input| input.alias == *source)?;
            if binding.point_id != sample.telemetry_id {
                return None;
            }
            let current = sample.value.as_f64()?;
            let previous = state
                .previous_by_alias
                .insert(source.clone(), sample.clone())?;
            let elapsed_ms = (sample.timestamp - previous.timestamp).num_milliseconds();
            let previous_value = previous.value.as_f64()?;
            if elapsed_ms <= 0 || *per_ms == 0 {
                return None;
            }
            Some(Ok((
                output,
                (current - previous_value) * *per_ms as f64 / elapsed_ms as f64,
            )))
        }
        _ => None,
    }) else {
        return Ok(Vec::new());
    };
    let (output_name, value) = result?;
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
        sample.quality,
        sample.timestamp,
    )
    .inherit_quality(sample)])
}

fn numeric_source(
    algorithm: &AlgorithmSpec,
    state: &AlgorithmState,
    sample: &TelemetrySample,
    source: &str,
) -> Option<f64> {
    let binding = algorithm
        .dsl
        .inputs
        .iter()
        .find(|input| input.alias == source)?;
    if binding.point_id != sample.telemetry_id {
        return None;
    }
    state
        .latest_by_alias
        .get(source)
        .and_then(|sample| sample.value.as_f64())
}

fn apply_debounce(
    algorithm: &AlgorithmSpec,
    state: &mut AlgorithmState,
    sample: &TelemetrySample,
) -> Result<Vec<TelemetrySample>> {
    let Some((source, stable_ms)) = algorithm.dsl.steps.iter().find_map(|step| match step {
        AlgorithmStep::Debounce { source, stable_ms } => Some((source, *stable_ms)),
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
    let pending = state
        .debounce_by_alias
        .entry(source.clone())
        .or_insert_with(|| DebounceState {
            value: sample.value.clone(),
            since: sample.timestamp,
        });
    if pending.value != sample.value {
        pending.value = sample.value.clone();
        pending.since = sample.timestamp;
        return Ok(Vec::new());
    }
    if (sample.timestamp - pending.since).num_milliseconds() < stable_ms as i64 {
        return Ok(Vec::new());
    }
    let Some(output) = algorithm.dsl.outputs.first() else {
        bail!("algorithm {} requires an output", algorithm.id);
    };
    if state.last_reported.get(&output.point_id) == Some(&sample.value) {
        return Ok(Vec::new());
    }
    state
        .last_reported
        .insert(output.point_id.clone(), sample.value.clone());
    Ok(vec![TelemetrySample::new(
        sample.device_id.clone(),
        output.point_id.clone(),
        sample.value.clone(),
        sample.quality,
        sample.timestamp,
    )
    .inherit_quality(sample)])
}

fn apply_duration_condition(
    algorithm: &AlgorithmSpec,
    state: &mut AlgorithmState,
    sample: &TelemetrySample,
) -> Result<Vec<TelemetrySample>> {
    let Some((source, operator, threshold, duration_ms, output_name)) =
        algorithm.dsl.steps.iter().find_map(|step| match step {
            AlgorithmStep::DurationCondition {
                source,
                operator,
                threshold,
                duration_ms,
                output,
            } => Some((source, *operator, *threshold, *duration_ms, output)),
            _ => None,
        })
    else {
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
    let Some(value) = sample.value.as_f64() else {
        return Ok(Vec::new());
    };
    if !compare(value, operator, threshold) {
        state.duration_by_alias.remove(source);
        return Ok(Vec::new());
    }

    let duration_state =
        state
            .duration_by_alias
            .entry(source.clone())
            .or_insert(DurationConditionState {
                since: sample.timestamp,
                emitted: false,
            });
    if sample.timestamp < duration_state.since {
        duration_state.since = sample.timestamp;
        duration_state.emitted = false;
    }
    let elapsed_ms = (sample.timestamp - duration_state.since).num_milliseconds() as u64;
    if duration_state.emitted || elapsed_ms < duration_ms {
        return Ok(Vec::new());
    }
    duration_state.emitted = true;

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
        sample.value.clone(),
        sample.quality,
        sample.timestamp,
    )
    .inherit_quality(sample)])
}

fn apply_threshold_rule(
    algorithm: &AlgorithmSpec,
    _state: &mut AlgorithmState,
    sample: &TelemetrySample,
) -> Result<(Vec<TelemetrySample>, Vec<EdgeRuntimeEvent>)> {
    let mut samples = Vec::new();
    let mut events = Vec::new();
    for step in &algorithm.dsl.steps {
        let (source, operator, threshold, event, route_outputs) = match step {
            AlgorithmStep::ThresholdRule {
                source,
                operator,
                threshold,
                event,
            } => (source, operator, threshold, Some(event), None),
            AlgorithmStep::ConditionalRoute {
                source,
                operator,
                threshold,
                matched_output,
                unmatched_output,
            } => (
                source,
                operator,
                threshold,
                None,
                Some((matched_output, unmatched_output)),
            ),
            _ => continue,
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
        let matched = compare(value, *operator, *threshold);
        if let Some((matched_output, unmatched_output)) = route_outputs {
            let output_name = if matched {
                matched_output
            } else {
                unmatched_output
            };
            let Some(output) = algorithm
                .dsl
                .outputs
                .iter()
                .find(|output| output.name == *output_name)
            else {
                bail!(
                    "algorithm {} references missing route output {}",
                    algorithm.id,
                    output_name
                );
            };
            samples.push(
                TelemetrySample::new(
                    sample.device_id.clone(),
                    output.point_id.clone(),
                    sample.value.clone(),
                    sample.quality,
                    sample.timestamp,
                )
                .inherit_quality(sample),
            );
        }
        if matched {
            let Some(event) = event else {
                continue;
            };
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
    Ok((samples, events))
}

fn value_changed(previous: &TelemetryValue, current: &TelemetryValue, threshold: f64) -> bool {
    match (previous.as_f64(), current.as_f64()) {
        (Some(previous), Some(current)) => (current - previous).abs() >= threshold,
        _ => previous != current,
    }
}

fn aggregate_value(
    function: &WindowAggregateFunction,
    values: &[f64],
) -> Option<(String, TelemetryValue)> {
    match function {
        WindowAggregateFunction::Avg { output } => non_empty(values).map(|values| {
            (
                output.clone(),
                TelemetryValue::Float(values.iter().sum::<f64>() / values.len() as f64),
            )
        }),
        WindowAggregateFunction::Min { output } => non_empty(values).map(|values| {
            (
                output.clone(),
                TelemetryValue::Float(values.iter().copied().fold(f64::INFINITY, f64::min)),
            )
        }),
        WindowAggregateFunction::Max { output } => non_empty(values).map(|values| {
            (
                output.clone(),
                TelemetryValue::Float(values.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
            )
        }),
        WindowAggregateFunction::Sum { output } => Some((
            output.clone(),
            TelemetryValue::Float(values.iter().sum::<f64>()),
        )),
        WindowAggregateFunction::Count { output } => {
            Some((output.clone(), TelemetryValue::Integer(values.len() as i64)))
        }
        WindowAggregateFunction::First { output } => values
            .first()
            .copied()
            .map(|value| (output.clone(), TelemetryValue::Float(value))),
        WindowAggregateFunction::Last { output } => values
            .last()
            .copied()
            .map(|value| (output.clone(), TelemetryValue::Float(value))),
    }
}

fn non_empty(values: &[f64]) -> Option<&[f64]> {
    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn evaluate_expression(expr: &str, values: &BTreeMap<String, TelemetrySample>) -> Result<f64> {
    let mut parser = ExpressionParser::new(expr, values);
    let value = parser.parse_expression()?;
    parser.skip_whitespace();
    if parser.position != parser.input.len() {
        bail!(
            "unexpected expression token near `{}`",
            &parser.input[parser.position..]
        );
    }
    Ok(value)
}

struct ExpressionParser<'a> {
    input: &'a str,
    position: usize,
    values: &'a BTreeMap<String, TelemetrySample>,
}

impl<'a> ExpressionParser<'a> {
    fn new(input: &'a str, values: &'a BTreeMap<String, TelemetrySample>) -> Self {
        Self {
            input,
            position: 0,
            values,
        }
    }

    fn parse_expression(&mut self) -> Result<f64> {
        let mut value = self.parse_term()?;
        loop {
            self.skip_whitespace();
            if self.consume('+') {
                value += self.parse_term()?;
            } else if self.consume('-') {
                value -= self.parse_term()?;
            } else {
                return Ok(value);
            }
        }
    }

    fn parse_term(&mut self) -> Result<f64> {
        let mut value = self.parse_factor()?;
        loop {
            self.skip_whitespace();
            if self.consume('*') {
                value *= self.parse_factor()?;
            } else if self.consume('/') {
                let divisor = self.parse_factor()?;
                if divisor == 0.0 {
                    bail!("expression division by zero");
                }
                value /= divisor;
            } else {
                return Ok(value);
            }
        }
    }

    fn parse_factor(&mut self) -> Result<f64> {
        self.skip_whitespace();
        if self.consume('-') {
            return Ok(-self.parse_factor()?);
        }
        if self.consume('(') {
            let value = self.parse_expression()?;
            self.skip_whitespace();
            if !self.consume(')') {
                bail!("expression is missing `)`");
            }
            return Ok(value);
        }
        if self
            .peek()
            .is_some_and(|character| character.is_ascii_digit() || character == '.')
        {
            return self.parse_number();
        }
        let identifier = self.parse_identifier()?;
        self.skip_whitespace();
        if self.consume('(') {
            let first = self.parse_expression()?;
            self.skip_whitespace();
            let second = if self.consume(',') {
                Some(self.parse_expression()?)
            } else {
                None
            };
            self.skip_whitespace();
            if !self.consume(')') {
                bail!("function {identifier} is missing `)`");
            }
            return match (identifier.as_str(), second) {
                ("abs", None) => Ok(first.abs()),
                ("round", None) => Ok(first.round()),
                ("floor", None) => Ok(first.floor()),
                ("ceil", None) => Ok(first.ceil()),
                ("sqrt", None) if first >= 0.0 => Ok(first.sqrt()),
                ("min", Some(second)) => Ok(first.min(second)),
                ("max", Some(second)) => Ok(first.max(second)),
                ("pow", Some(second)) => Ok(first.powf(second)),
                _ => bail!("unsupported expression function `{identifier}`"),
            };
        }
        self.values
            .get(&identifier)
            .and_then(|sample| sample.value.as_f64())
            .ok_or_else(|| {
                anyhow::anyhow!("expression references missing numeric alias `{identifier}`")
            })
    }

    fn parse_number(&mut self) -> Result<f64> {
        let start = self.position;
        while self.peek().is_some_and(|character| {
            character.is_ascii_digit() || matches!(character, '.' | 'e' | 'E' | '+' | '-')
        }) {
            if self.position > start && matches!(self.peek(), Some('+') | Some('-')) {
                let previous = self.input.as_bytes()[self.position - 1] as char;
                if !matches!(previous, 'e' | 'E') {
                    break;
                }
            }
            self.advance();
        }
        self.input[start..self.position]
            .parse::<f64>()
            .map_err(|_| {
                anyhow::anyhow!(
                    "invalid numeric literal `{}`",
                    &self.input[start..self.position]
                )
            })
    }

    fn parse_identifier(&mut self) -> Result<String> {
        let start = self.position;
        while self
            .peek()
            .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
        {
            self.advance();
        }
        if start == self.position {
            bail!("expected expression value");
        }
        Ok(self.input[start..self.position].to_string())
    }

    fn skip_whitespace(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.advance();
        }
    }

    fn consume(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.input[self.position..].chars().next()
    }
    fn advance(&mut self) {
        if let Some(character) = self.peek() {
            self.position += character.len_utf8();
        }
    }
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

const fn data_quality_rank(quality: DataQuality) -> u8 {
    match quality {
        DataQuality::Good => 0,
        DataQuality::Uncertain => 1,
        DataQuality::Bad => 2,
    }
}

fn map_event_severity(severity: EventSeverity) -> RuntimeEventSeverity {
    match severity {
        EventSeverity::Info => RuntimeEventSeverity::Info,
        EventSeverity::Warning => RuntimeEventSeverity::Warning,
        EventSeverity::Critical => RuntimeEventSeverity::Critical,
    }
}
