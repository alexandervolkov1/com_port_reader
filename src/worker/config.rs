use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkerConfig {
    poll_interval: Duration,
}

impl WorkerConfig {
    pub fn new(poll_interval: Duration) -> Self {
        assert!(
            !poll_interval.is_zero(),
            "poll interval must be greater than zero",
        );

        Self { poll_interval }
    }

    pub const fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self::new(Duration::from_secs(1))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::WorkerConfig;

    #[test]
    fn stores_poll_interval() {
        let config = WorkerConfig::new(Duration::from_millis(100));

        assert_eq!(config.poll_interval(), Duration::from_millis(100),);
    }

    #[test]
    #[should_panic(expected = "poll interval must be greater than zero")]
    fn rejects_zero_interval() {
        WorkerConfig::new(Duration::ZERO);
    }
}
