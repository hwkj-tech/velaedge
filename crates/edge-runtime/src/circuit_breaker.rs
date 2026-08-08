use std::{
    collections::BTreeMap,
    error::Error,
    fmt,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use edge_core::{ProtocolCircuitBreakerConfig, ProtocolCircuitState, ProtocolConnection};

#[derive(Clone, Debug, Default)]
pub struct ProtocolCircuitBreakerRegistry {
    inner: Arc<Mutex<BTreeMap<String, ProtocolCircuitBreaker>>>,
}

impl ProtocolCircuitBreakerRegistry {
    pub(crate) fn configure(&self, connections: &[ProtocolConnection]) {
        let mut breakers = self.lock();
        for connection in connections {
            match breakers.get(&connection.connection_id) {
                Some(breaker) if breaker.config == connection.circuit_breaker => {}
                _ => {
                    breakers.insert(
                        connection.connection_id.clone(),
                        ProtocolCircuitBreaker::new(connection.circuit_breaker.clone()),
                    );
                }
            }
        }
    }

    pub(crate) fn allow_request(
        &self,
        connection_id: &str,
        now: Instant,
    ) -> Result<CircuitBreakerSnapshot, ProtocolCircuitOpenError> {
        let mut breakers = self.lock();
        let breaker = breakers
            .get_mut(connection_id)
            .expect("configured connection must have a circuit breaker");
        breaker.allow_request(connection_id, now)?;
        Ok(breaker.snapshot())
    }

    pub(crate) fn record_success(&self, connection_id: &str) -> CircuitBreakerSnapshot {
        let mut breakers = self.lock();
        let breaker = breakers
            .get_mut(connection_id)
            .expect("configured connection must have a circuit breaker");
        breaker.record_success();
        breaker.snapshot()
    }

    pub(crate) fn record_failure(
        &self,
        connection_id: &str,
        now: Instant,
    ) -> CircuitBreakerSnapshot {
        let mut breakers = self.lock();
        let breaker = breakers
            .get_mut(connection_id)
            .expect("configured connection must have a circuit breaker");
        breaker.record_failure(now);
        breaker.snapshot()
    }

    pub(crate) fn snapshot(&self, connection_id: &str) -> CircuitBreakerSnapshot {
        self.lock()
            .get(connection_id)
            .expect("configured connection must have a circuit breaker")
            .snapshot()
    }

    fn lock(&self) -> MutexGuard<'_, BTreeMap<String, ProtocolCircuitBreaker>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CircuitBreakerSnapshot {
    pub state: ProtocolCircuitState,
    pub consecutive_failures: u32,
    pub open_count: u64,
    pub rejected_count: u64,
}

#[derive(Debug)]
pub(crate) struct ProtocolCircuitOpenError {
    connection_id: String,
    retry_after_ms: u64,
}

impl ProtocolCircuitOpenError {
    fn new(connection_id: impl Into<String>, retry_after_ms: u64) -> Self {
        Self {
            connection_id: connection_id.into(),
            retry_after_ms,
        }
    }
}

impl fmt::Display for ProtocolCircuitOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "protocol connection {} circuit breaker is open; retry after {} ms",
            self.connection_id, self.retry_after_ms
        )
    }
}

impl Error for ProtocolCircuitOpenError {}

pub(crate) fn is_circuit_open_error(error: &anyhow::Error) -> bool {
    error.downcast_ref::<ProtocolCircuitOpenError>().is_some()
}

#[derive(Clone, Debug)]
pub(crate) struct ProtocolCircuitBreaker {
    config: ProtocolCircuitBreakerConfig,
    state: ProtocolCircuitState,
    consecutive_failures: u32,
    half_open_successes: u32,
    opened_at: Option<Instant>,
    open_count: u64,
    rejected_count: u64,
}

impl ProtocolCircuitBreaker {
    pub fn new(config: ProtocolCircuitBreakerConfig) -> Self {
        Self {
            config,
            state: ProtocolCircuitState::Closed,
            consecutive_failures: 0,
            half_open_successes: 0,
            opened_at: None,
            open_count: 0,
            rejected_count: 0,
        }
    }

    pub fn allow_request(
        &mut self,
        connection_id: &str,
        now: Instant,
    ) -> Result<(), ProtocolCircuitOpenError> {
        if !self.config.enabled {
            self.close();
            return Ok(());
        }

        if self.state == ProtocolCircuitState::Open {
            let elapsed = self
                .opened_at
                .map(|opened_at| now.saturating_duration_since(opened_at))
                .unwrap_or_default();
            let open_duration = Duration::from_millis(self.config.open_duration_ms);
            if elapsed >= open_duration {
                self.state = ProtocolCircuitState::HalfOpen;
                self.half_open_successes = 0;
                return Ok(());
            }

            self.rejected_count = self.rejected_count.saturating_add(1);
            let retry_after = open_duration.saturating_sub(elapsed);
            return Err(ProtocolCircuitOpenError::new(
                connection_id,
                duration_millis_ceil(retry_after),
            ));
        }

        Ok(())
    }

    pub fn record_success(&mut self) {
        if !self.config.enabled {
            self.close();
            return;
        }

        match self.state {
            ProtocolCircuitState::Closed => {
                self.consecutive_failures = 0;
            }
            ProtocolCircuitState::HalfOpen => {
                self.half_open_successes = self.half_open_successes.saturating_add(1);
                if self.half_open_successes >= self.config.half_open_success_threshold {
                    self.close();
                }
            }
            ProtocolCircuitState::Open => {}
        }
    }

    pub fn record_failure(&mut self, now: Instant) {
        if !self.config.enabled {
            self.close();
            return;
        }

        match self.state {
            ProtocolCircuitState::Closed => {
                self.consecutive_failures = self.consecutive_failures.saturating_add(1);
                if self.consecutive_failures >= self.config.failure_threshold {
                    self.open(now);
                }
            }
            ProtocolCircuitState::HalfOpen => self.open(now),
            ProtocolCircuitState::Open => {}
        }
    }

    pub fn snapshot(&self) -> CircuitBreakerSnapshot {
        CircuitBreakerSnapshot {
            state: self.state,
            consecutive_failures: self.consecutive_failures,
            open_count: self.open_count,
            rejected_count: self.rejected_count,
        }
    }

    fn open(&mut self, now: Instant) {
        self.state = ProtocolCircuitState::Open;
        self.opened_at = Some(now);
        self.half_open_successes = 0;
        self.open_count = self.open_count.saturating_add(1);
    }

    fn close(&mut self) {
        self.state = ProtocolCircuitState::Closed;
        self.consecutive_failures = 0;
        self.half_open_successes = 0;
        self.opened_at = None;
    }
}

fn duration_millis_ceil(duration: Duration) -> u64 {
    let millis = duration.as_millis();
    let rounded = if duration.subsec_nanos().is_multiple_of(1_000_000) {
        millis
    } else {
        millis.saturating_add(1)
    };
    rounded.min(u64::MAX as u128) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> ProtocolCircuitBreakerConfig {
        ProtocolCircuitBreakerConfig {
            enabled: true,
            failure_threshold: 2,
            open_duration_ms: 1_000,
            half_open_success_threshold: 2,
        }
    }

    #[test]
    fn opens_rejects_probes_and_closes_after_half_open_successes() {
        let started = Instant::now();
        let mut breaker = ProtocolCircuitBreaker::new(config());

        breaker.record_failure(started);
        assert_eq!(breaker.snapshot().state, ProtocolCircuitState::Closed);
        breaker.record_failure(started + Duration::from_millis(10));
        assert_eq!(breaker.snapshot().state, ProtocolCircuitState::Open);
        assert_eq!(breaker.snapshot().open_count, 1);

        let error = breaker
            .allow_request("modbus-main", started + Duration::from_millis(500))
            .unwrap_err();
        assert!(error.to_string().contains("retry after 510 ms"));
        assert_eq!(breaker.snapshot().rejected_count, 1);

        breaker
            .allow_request("modbus-main", started + Duration::from_millis(1_010))
            .unwrap();
        assert_eq!(breaker.snapshot().state, ProtocolCircuitState::HalfOpen);
        breaker.record_success();
        assert_eq!(breaker.snapshot().state, ProtocolCircuitState::HalfOpen);
        breaker.record_success();
        assert_eq!(breaker.snapshot().state, ProtocolCircuitState::Closed);
        assert_eq!(breaker.snapshot().consecutive_failures, 0);
    }

    #[test]
    fn failed_half_open_probe_reopens_the_breaker() {
        let started = Instant::now();
        let mut breaker = ProtocolCircuitBreaker::new(config());
        breaker.record_failure(started);
        breaker.record_failure(started);
        breaker
            .allow_request("modbus-main", started + Duration::from_secs(1))
            .unwrap();

        breaker.record_failure(started + Duration::from_secs(1));

        assert_eq!(breaker.snapshot().state, ProtocolCircuitState::Open);
        assert_eq!(breaker.snapshot().open_count, 2);
    }
}
