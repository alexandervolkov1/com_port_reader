use std::sync::{Arc, Mutex};

use super::SignalSeries;

#[derive(Clone, Default)]
pub struct SeriesStore {
    inner: Arc<Mutex<Vec<SignalSeries>>>,
}

impl SeriesStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with<R>(&self, operation: impl FnOnce(&[SignalSeries]) -> R) -> R {
        let series = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        operation(&series)
    }

    pub fn with_mut<R>(&self, operation: impl FnOnce(&mut Vec<SignalSeries>) -> R) -> R {
        let mut series = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        operation(&mut series)
    }

    pub fn push(&self, series: SignalSeries) {
        self.with_mut(|all_series| {
            all_series.push(series);
        });
    }

    pub fn clear(&self) {
        self.with_mut(Vec::clear);
    }
}
