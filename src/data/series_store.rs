use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::{SeriesId, SeriesMetadata, Signal, SignalSeries};

struct SeriesStoreInner {
    series: Mutex<Vec<SignalSeries>>,
    next_id: AtomicU64,
}

#[derive(Clone)]
pub struct SeriesStore {
    inner: Arc<SeriesStoreInner>,
}

impl SeriesStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with<R>(&self, operation: impl FnOnce(&[SignalSeries]) -> R) -> R {
        let series = self
            .inner
            .series
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        operation(&series)
    }

    pub fn with_mut<R>(&self, operation: impl FnOnce(&mut Vec<SignalSeries>) -> R) -> R {
        let mut series = self
            .inner
            .series
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        operation(&mut series)
    }

    pub fn add_signal(&self, signal: Signal) -> SeriesId {
        let id = SeriesId::new(self.inner.next_id.fetch_add(1, Ordering::Relaxed));

        self.with_mut(|series| {
            series.push(SignalSeries::new(id, signal));
        });

        id
    }

    pub fn clear(&self) {
        self.with_mut(Vec::clear);
    }

    pub fn metadata(&self) -> Vec<SeriesMetadata> {
        self.with(|series| series.iter().map(SeriesMetadata::from).collect())
    }

    pub fn set_visibility(&self, id: SeriesId, visible: bool) -> bool {
        self.with_mut(|series| {
            let Some(series) = series.iter_mut().find(|series| series.id == id) else {
                return false;
            };

            series.visible = visible;
            true
        })
    }

    pub fn remove_series(&self, id: SeriesId) -> bool {
        self.with_mut(|series| {
            let Some(index) = series.iter().position(|series| series.id == id) else {
                return false;
            };

            series.remove(index);
            true
        })
    }
}

impl Default for SeriesStore {
    fn default() -> Self {
        Self {
            inner: Arc::new(SeriesStoreInner {
                series: Mutex::new(Vec::new()),
                next_id: AtomicU64::new(1),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SeriesStore;
    use crate::data::Signal;

    #[test]
    fn assigns_unique_ids() {
        let store = SeriesStore::new();

        let first_id = store.add_signal(Signal::Constant { value: 1.0 });

        let second_id = store.add_signal(Signal::Constant { value: 2.0 });

        assert_ne!(first_id, second_id);

        let stored_ids = store.with(|series| {
            series
                .iter()
                .map(|signal_series| signal_series.id)
                .collect::<Vec<_>>()
        });

        assert_eq!(stored_ids, vec![first_id, second_id]);
    }

    #[test]
    fn does_not_reuse_ids_after_clear() {
        let store = SeriesStore::new();

        let first_id = store.add_signal(Signal::Constant { value: 1.0 });

        store.clear();

        let second_id = store.add_signal(Signal::Constant { value: 2.0 });

        assert_ne!(first_id, second_id);
    }

    #[test]
    fn changes_visibility_by_id() {
        let store = SeriesStore::new();

        let id = store.add_signal(Signal::Constant { value: 1.0 });

        assert!(store.set_visibility(id, false));

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].id, id);
        assert!(!metadata[0].visible);
    }

    #[test]
    fn removes_series_by_id() {
        let store = SeriesStore::new();

        let first_id = store.add_signal(Signal::Constant { value: 1.0 });

        let second_id = store.add_signal(Signal::Constant { value: 2.0 });

        assert!(store.remove_series(first_id));

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].id, second_id);
    }

    #[test]
    fn reports_missing_series() {
        let store = SeriesStore::new();

        let id = store.add_signal(Signal::Constant { value: 1.0 });

        assert!(store.remove_series(id));
        assert!(!store.remove_series(id));
        assert!(!store.set_visibility(id, false));
    }
}
