use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

pub const LLM_REQUEST_TIMEOUT_SECS: u64 = 8;
const LLM_MAX_ATTEMPTS: u32 = 2; // initial + 1 retry
const LLM_BACKOFF_BASE_MS: u64 = 250;
const LLM_BACKOFF_JITTER_MAX_MS: u64 = 100;
const LLM_CIRCUIT_FAILURE_THRESHOLD: u32 = 3;
const LLM_CIRCUIT_COOLDOWN_SECS: u64 = 90;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LlmGuardErrorCode {
    Timeout,
    NetworkError,
    Http5xx,
    BadResponse,
    CircuitOpen,
}

impl LlmGuardErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Timeout => "LLM_TIMEOUT",
            Self::NetworkError => "LLM_NETWORK_ERROR",
            Self::Http5xx => "LLM_5XX",
            Self::BadResponse => "LLM_BAD_RESPONSE",
            Self::CircuitOpen => "LLM_CIRCUIT_OPEN",
        }
    }

    fn retryable(self) -> bool {
        matches!(self, Self::Timeout | Self::NetworkError | Self::Http5xx)
    }
}

pub trait LlmGuardClassifiedError {
    fn llm_error_code(&self) -> LlmGuardErrorCode;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmGuardStatus {
    pub circuit_open: bool,
    pub circuit_open_until_ms: Option<u64>,
    pub consecutive_failures: u32,
}

#[derive(Debug, Clone)]
pub struct LlmGuardOutcome<T> {
    pub value: Option<T>,
    pub error_code: Option<LlmGuardErrorCode>,
    pub attempts: u32,
    pub circuit_open: bool,
    pub circuit_open_until_ms: Option<u64>,
}

#[derive(Debug, Default)]
struct LlmGuardState {
    consecutive_failures: u32,
    open_until: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct LlmGuard {
    state: Mutex<LlmGuardState>,
}

impl LlmGuard {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(LlmGuardState::default()),
        }
    }

    pub fn status(&self) -> LlmGuardStatus {
        let mut state = self.state.lock();
        let now = Instant::now();
        if state.open_until.is_some_and(|until| now >= until) {
            state.open_until = None;
        }
        LlmGuardStatus {
            circuit_open: state.open_until.is_some(),
            circuit_open_until_ms: state.open_until.map(|until| {
                now_epoch_ms()
                    .saturating_add(until.saturating_duration_since(now).as_millis() as u64)
            }),
            consecutive_failures: state.consecutive_failures,
        }
    }

    pub fn execute<T, E, F>(&self, mut run: F) -> LlmGuardOutcome<T>
    where
        E: LlmGuardClassifiedError,
        F: FnMut(Duration) -> Result<T, E>,
    {
        let now = Instant::now();
        {
            let mut state = self.state.lock();
            if state.open_until.is_some_and(|until| now >= until) {
                state.open_until = None;
            }
            if let Some(until) = state.open_until {
                return LlmGuardOutcome {
                    value: None,
                    error_code: Some(LlmGuardErrorCode::CircuitOpen),
                    attempts: 0,
                    circuit_open: true,
                    circuit_open_until_ms:
                        Some(
                            now_epoch_ms().saturating_add(
                                until.saturating_duration_since(now).as_millis() as u64,
                            ),
                        ),
                };
            }
        }

        let timeout = Duration::from_secs(LLM_REQUEST_TIMEOUT_SECS);
        let mut last_error: Option<LlmGuardErrorCode> = None;

        for attempt in 1..=LLM_MAX_ATTEMPTS {
            match run(timeout) {
                Ok(value) => {
                    let mut state = self.state.lock();
                    state.consecutive_failures = 0;
                    state.open_until = None;
                    return LlmGuardOutcome {
                        value: Some(value),
                        error_code: None,
                        attempts: attempt,
                        circuit_open: false,
                        circuit_open_until_ms: None,
                    };
                }
                Err(err) => {
                    let code = err.llm_error_code();
                    last_error = Some(code);

                    let should_retry = attempt < LLM_MAX_ATTEMPTS && code.retryable();
                    if should_retry {
                        let jitter = now_epoch_ms() % (LLM_BACKOFF_JITTER_MAX_MS + 1);
                        thread::sleep(Duration::from_millis(LLM_BACKOFF_BASE_MS + jitter));
                        continue;
                    }

                    let mut state = self.state.lock();
                    state.consecutive_failures = state.consecutive_failures.saturating_add(1);
                    if state.consecutive_failures >= LLM_CIRCUIT_FAILURE_THRESHOLD {
                        state.open_until =
                            Some(Instant::now() + Duration::from_secs(LLM_CIRCUIT_COOLDOWN_SECS));
                    }
                    let now = Instant::now();
                    let circuit_open = state.open_until.is_some();
                    let circuit_open_until_ms = state.open_until.map(|until| {
                        now_epoch_ms()
                            .saturating_add(until.saturating_duration_since(now).as_millis() as u64)
                    });
                    return LlmGuardOutcome {
                        value: None,
                        error_code: Some(code),
                        attempts: attempt,
                        circuit_open,
                        circuit_open_until_ms,
                    };
                }
            }
        }

        LlmGuardOutcome {
            value: None,
            error_code: last_error.or(Some(LlmGuardErrorCode::BadResponse)),
            attempts: LLM_MAX_ATTEMPTS,
            circuit_open: false,
            circuit_open_until_ms: None,
        }
    }

    #[cfg(test)]
    fn force_reset(&self) {
        let mut state = self.state.lock();
        state.consecutive_failures = 0;
        state.open_until = None;
    }
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy)]
    enum FakeErr {
        Timeout,
        Bad,
    }

    impl LlmGuardClassifiedError for FakeErr {
        fn llm_error_code(&self) -> LlmGuardErrorCode {
            match self {
                Self::Timeout => LlmGuardErrorCode::Timeout,
                Self::Bad => LlmGuardErrorCode::BadResponse,
            }
        }
    }

    #[test]
    fn retries_once_then_succeeds() {
        let guard = LlmGuard::new();
        let mut calls = 0_u32;
        let outcome = guard.execute::<_, FakeErr, _>(|_| {
            calls = calls.saturating_add(1);
            if calls == 1 {
                Err(FakeErr::Timeout)
            } else {
                Ok("ok")
            }
        });
        assert_eq!(outcome.value, Some("ok"));
        assert_eq!(outcome.attempts, 2);
        assert_eq!(outcome.error_code, None);
    }

    #[test]
    fn opens_circuit_after_threshold() {
        let guard = LlmGuard::new();
        guard.force_reset();

        for _ in 0..3 {
            let outcome = guard.execute::<(), FakeErr, _>(|_| Err(FakeErr::Bad));
            assert_eq!(outcome.value, None);
        }

        let blocked = guard.execute::<(), FakeErr, _>(|_| Ok(()));
        assert_eq!(blocked.error_code, Some(LlmGuardErrorCode::CircuitOpen));
        assert!(blocked.circuit_open);
    }

    #[test]
    fn failure_outcome_keeps_value_none_for_fail_open_callers() {
        let guard = LlmGuard::new();
        let outcome = guard.execute::<&'static str, FakeErr, _>(|_| Err(FakeErr::Bad));
        assert!(outcome.value.is_none());
        assert_eq!(outcome.error_code, Some(LlmGuardErrorCode::BadResponse));
    }
}
