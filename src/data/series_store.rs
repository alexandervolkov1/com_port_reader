use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::{
    NewSeries, SeriesId, SeriesMetadata, SeriesNameError, SeriesSource, SignalSeries,
    SignalValidationError, series_name::normalize_series_name,
};

struct SeriesStoreInner {
    series: Mutex<Vec<SignalSeries>>,
    next_id: AtomicU64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddSeriesError {
    InvalidSignal(SignalValidationError),
    InvalidName(SeriesNameError),
    EmptySerialCommand,
    SerialCommandContainsLineBreak,
}

impl std::fmt::Display for AddSeriesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSignal(error) => error.fmt(formatter),

            Self::InvalidName(error) => error.fmt(formatter),

            Self::EmptySerialCommand => formatter.write_str("Serial command cannot be empty"),

            Self::SerialCommandContainsLineBreak => {
                formatter.write_str("Serial command cannot contain a line break")
            }
        }
    }
}

impl std::error::Error for AddSeriesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidSignal(error) => Some(error),
            Self::InvalidName(error) => Some(error),

            Self::EmptySerialCommand | Self::SerialCommandContainsLineBreak => None,
        }
    }
}

impl From<SignalValidationError> for AddSeriesError {
    fn from(error: SignalValidationError) -> Self {
        Self::InvalidSignal(error)
    }
}

impl From<SeriesNameError> for AddSeriesError {
    fn from(error: SeriesNameError) -> Self {
        Self::InvalidName(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameSeriesError {
    NotFound(String),
    InvalidName(SeriesNameError),
}

impl std::fmt::Display for RenameSeriesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(name) => {
                write!(formatter, "Series '{name}' not found")
            }

            Self::InvalidName(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for RenameSeriesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NotFound(_) => None,
            Self::InvalidName(error) => Some(error),
        }
    }
}

impl From<SeriesNameError> for RenameSeriesError {
    fn from(error: SeriesNameError) -> Self {
        Self::InvalidName(error)
    }
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

    pub fn add_series(&self, new_series: NewSeries) -> Result<SeriesId, AddSeriesError> {
        let (source, requested_name) = new_series.into_source_parts();

        let source = normalize_series_source(source)?;

        self.with_mut(|series| {
            let custom_name = match requested_name {
                Some(name) => {
                    let name = normalize_series_name(&name)?;

                    if contains_name(series, name) {
                        return Err(SeriesNameError::Duplicate(name.to_owned()).into());
                    }

                    Some(name.to_owned())
                }

                None => None,
            };

            let id = SeriesId::new(self.inner.next_id.fetch_add(1, Ordering::Relaxed));

            let name = custom_name
                .unwrap_or_else(|| generate_default_name(series, source.default_name_prefix(), id));

            series.push(SignalSeries::new(id, name, source));

            Ok(id)
        })
    }

    pub fn remove_series_by_name(&self, name: &str) -> Option<SeriesId> {
        self.with_mut(|series| {
            let index = series.iter().position(|series| series.name == name)?;

            Some(series.remove(index).id)
        })
    }

    pub fn rename_series(
        &self,
        current_name: &str,
        new_name: &str,
    ) -> Result<SeriesId, RenameSeriesError> {
        self.with_mut(|series| {
            let Some(index) = series.iter().position(|series| series.name == current_name) else {
                return Err(RenameSeriesError::NotFound(current_name.to_owned()));
            };

            let new_name = normalize_series_name(new_name)?;

            let id = series[index].id;

            if series[index].name == new_name {
                return Ok(id);
            }

            if contains_name(series, new_name) {
                return Err(SeriesNameError::Duplicate(new_name.to_owned()).into());
            }

            series[index].name.clear();
            series[index].name.push_str(new_name);

            Ok(id)
        })
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

fn normalize_series_source(source: SeriesSource) -> Result<SeriesSource, AddSeriesError> {
    match source {
        SeriesSource::Generated(signal) => {
            signal.validate()?;

            Ok(SeriesSource::Generated(signal))
        }

        SeriesSource::SerialCommand { command } => {
            if command.contains('\r') || command.contains('\n') {
                return Err(AddSeriesError::SerialCommandContainsLineBreak);
            }

            let command = command.trim();

            if command.is_empty() {
                return Err(AddSeriesError::EmptySerialCommand);
            }

            Ok(SeriesSource::SerialCommand {
                command: command.to_owned(),
            })
        }
    }
}

fn contains_name(series: &[SignalSeries], name: &str) -> bool {
    series.iter().any(|series| series.name == name)
}

fn generate_default_name(series: &[SignalSeries], prefix: &str, id: SeriesId) -> String {
    let base_name = format!("{prefix}{id}");

    if !contains_name(series, &base_name) {
        return base_name;
    }

    let mut suffix = 2_u64;

    loop {
        let candidate = format!("{base_name}_{suffix}");

        if !contains_name(series, &candidate) {
            return candidate;
        }

        suffix += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::{RenameSeriesError, SeriesStore};

    use crate::data::{AddSeriesError, NewSeries, SeriesId, SeriesNameError, SeriesSource, Signal};

    fn add_unnamed(store: &SeriesStore, signal: Signal) -> SeriesId {
        store.add_series(NewSeries::unnamed(signal)).unwrap()
    }

    #[test]
    fn assigns_unique_ids() {
        let store = SeriesStore::new();

        let first_id = add_unnamed(&store, Signal::Constant { value: 1.0 });

        let second_id = add_unnamed(&store, Signal::Constant { value: 2.0 });

        assert_ne!(first_id, second_id);

        let stored_ids =
            store.with(|series| series.iter().map(|series| series.id).collect::<Vec<_>>());

        assert_eq!(stored_ids, vec![first_id, second_id],);
    }

    #[test]
    fn does_not_reuse_ids_after_clear() {
        let store = SeriesStore::new();

        let first_id = add_unnamed(&store, Signal::Constant { value: 1.0 });

        store.clear();

        let second_id = add_unnamed(&store, Signal::Constant { value: 2.0 });

        assert_ne!(first_id, second_id);
    }

    #[test]
    fn changes_visibility_by_id() {
        let store = SeriesStore::new();

        let id = add_unnamed(&store, Signal::Constant { value: 1.0 });

        assert!(store.set_visibility(id, false));

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].id, id);
        assert!(!metadata[0].visible);
    }

    #[test]
    fn removes_series_by_id() {
        let store = SeriesStore::new();

        let first_id = add_unnamed(&store, Signal::Constant { value: 1.0 });

        let second_id = add_unnamed(&store, Signal::Constant { value: 2.0 });

        assert!(store.remove_series(first_id));

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].id, second_id);
    }

    #[test]
    fn reports_missing_series() {
        let store = SeriesStore::new();

        let id = add_unnamed(&store, Signal::Constant { value: 1.0 });

        assert!(store.remove_series(id));
        assert!(!store.remove_series(id));
        assert!(!store.set_visibility(id, false));
    }

    #[test]
    fn generates_unique_default_names() {
        let store = SeriesStore::new();

        add_unnamed(
            &store,
            Signal::SineWave {
                amplitude: 1.0,
                period: 10.0,
                phase: 0.0,
            },
        );

        add_unnamed(
            &store,
            Signal::SquareWave {
                amplitude: 1.0,
                period: 10.0,
                duty_cycle: 0.5,
            },
        );

        let names = store.with(|series| {
            series
                .iter()
                .map(|series| series.name.clone())
                .collect::<Vec<_>>()
        });

        assert_eq!(names, vec!["sine1", "square2"],);
    }

    #[test]
    fn does_not_reuse_default_names_after_clear() {
        let store = SeriesStore::new();

        add_unnamed(&store, Signal::Constant { value: 1.0 });

        store.clear();

        add_unnamed(&store, Signal::Constant { value: 2.0 });

        let names = store.with(|series| {
            series
                .iter()
                .map(|series| series.name.clone())
                .collect::<Vec<_>>()
        });

        assert_eq!(names, vec!["constant2"]);
    }

    #[test]
    fn accepts_custom_name() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::named(
                Signal::Constant { value: 1.0 },
                "temperature",
            ))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata[0].name, "temperature",);
    }

    #[test]
    fn trims_custom_name() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::named(
                Signal::Constant { value: 1.0 },
                "  temperature  ",
            ))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata[0].name, "temperature",);
    }

    #[test]
    fn rejects_duplicate_name() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::named(
                Signal::Constant { value: 1.0 },
                "temperature",
            ))
            .unwrap();

        let result = store.add_series(NewSeries::named(
            Signal::Constant { value: 2.0 },
            "temperature",
        ));

        assert_eq!(
            result,
            Err(AddSeriesError::InvalidName(SeriesNameError::Duplicate(
                "temperature".to_owned(),
            ),)),
        );
    }

    #[test]
    fn rejects_empty_name() {
        let store = SeriesStore::new();

        let result = store.add_series(NewSeries::named(Signal::Constant { value: 1.0 }, "   "));

        assert_eq!(
            result,
            Err(AddSeriesError::InvalidName(SeriesNameError::Empty,)),
        );
    }

    #[test]
    fn rejects_name_with_whitespace() {
        let store = SeriesStore::new();

        let result = store.add_series(NewSeries::named(
            Signal::Constant { value: 1.0 },
            "room temperature",
        ));

        assert_eq!(
            result,
            Err(AddSeriesError::InvalidName(
                SeriesNameError::ContainsWhitespace,
            )),
        );
    }

    #[test]
    fn removes_series_by_name() {
        let store = SeriesStore::new();

        let id = store
            .add_series(NewSeries::named(
                Signal::Constant { value: 1.0 },
                "temperature",
            ))
            .unwrap();

        assert_eq!(store.remove_series_by_name("temperature",), Some(id),);

        assert!(store.metadata().is_empty());

        assert_eq!(store.remove_series_by_name("temperature",), None,);
    }

    #[test]
    fn renames_series_without_changing_id() {
        let store = SeriesStore::new();

        let id = store
            .add_series(NewSeries::named(
                Signal::Constant { value: 1.0 },
                "temperature",
            ))
            .unwrap();

        assert_eq!(
            store.rename_series("temperature", "room_temperature",),
            Ok(id),
        );

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].id, id);

        assert_eq!(metadata[0].name, "room_temperature",);
    }

    #[test]
    fn accepts_unchanged_name_during_rename() {
        let store = SeriesStore::new();

        let id = store
            .add_series(NewSeries::named(
                Signal::Constant { value: 1.0 },
                "temperature",
            ))
            .unwrap();

        assert_eq!(store.rename_series("temperature", "temperature",), Ok(id),);
    }

    #[test]
    fn rejects_duplicate_name_during_rename() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::named(Signal::Constant { value: 1.0 }, "first"))
            .unwrap();

        store
            .add_series(NewSeries::named(Signal::Constant { value: 2.0 }, "second"))
            .unwrap();

        let result = store.rename_series("first", "second");

        assert_eq!(
            result,
            Err(RenameSeriesError::InvalidName(SeriesNameError::Duplicate(
                "second".to_owned(),
            ),)),
        );
    }

    #[test]
    fn reports_missing_series_during_rename() {
        let store = SeriesStore::new();

        let result = store.rename_series("missing", "new_name");

        assert_eq!(
            result,
            Err(RenameSeriesError::NotFound("missing".to_owned(),)),
        );
    }

    #[test]
    fn stores_serial_command_series() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::serial_command("  get  ", "random_walk"))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);

        assert_eq!(metadata[0].name, "random_walk",);

        assert_eq!(
            metadata[0].source,
            SeriesSource::SerialCommand {
                command: "get".to_owned(),
            },
        );
    }

    #[test]
    fn rejects_empty_serial_command() {
        let store = SeriesStore::new();

        let result = store.add_series(NewSeries::serial_command("   ", "random_walk"));

        assert_eq!(result, Err(AddSeriesError::EmptySerialCommand),);
    }

    #[test]
    fn rejects_serial_command_with_line_break() {
        let store = SeriesStore::new();

        let result = store.add_series(NewSeries::serial_command("get\nnext", "random_walk"));

        assert_eq!(result, Err(AddSeriesError::SerialCommandContainsLineBreak,),);
    }
}
