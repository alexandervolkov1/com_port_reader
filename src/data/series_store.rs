use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use super::{
    NewSeries, Series, SeriesId, SeriesMetadata, SeriesNameError, SeriesSource,
    series_name::normalize_series_name,
};

struct SeriesStoreInner {
    series: Mutex<Vec<Series>>,
    next_id: AtomicU64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddSeriesError {
    InvalidName(SeriesNameError),
    EmptySerialCommand,
    SerialCommandContainsLineBreak,
    InvalidMetakonScale,
}

impl std::fmt::Display for AddSeriesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(error) => error.fmt(formatter),

            Self::EmptySerialCommand => formatter.write_str("Serial command cannot be empty"),

            Self::SerialCommandContainsLineBreak => {
                formatter.write_str("Serial command cannot contain a line break")
            }

            Self::InvalidMetakonScale => {
                formatter.write_str("Metakon scale must be finite and greater than zero")
            }
        }
    }
}

impl std::error::Error for AddSeriesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidName(error) => Some(error),

            Self::EmptySerialCommand
            | Self::SerialCommandContainsLineBreak
            | Self::InvalidMetakonScale => None,
        }
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

    pub fn with<R>(&self, operation: impl FnOnce(&[Series]) -> R) -> R {
        let series = self
            .inner
            .series
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        operation(&series)
    }

    pub fn with_mut<R>(&self, operation: impl FnOnce(&mut Vec<Series>) -> R) -> R {
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

            series.push(Series::new(id, name, source));

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

        SeriesSource::Metakon {
            device,
            channel,
            register,
            value_type,
            scale,
        } => {
            if !scale.is_finite() || scale <= 0.0 {
                return Err(AddSeriesError::InvalidMetakonScale);
            }

            Ok(SeriesSource::Metakon {
                device,
                channel,
                register,
                value_type,
                scale,
            })
        }
    }
}

fn contains_name(series: &[Series], name: &str) -> bool {
    series.iter().any(|series| series.name == name)
}

fn generate_default_name(series: &[Series], prefix: &str, id: SeriesId) -> String {
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

    use crate::data::{
        AddSeriesError, MetakonValueType, NewSeries, SeriesId, SeriesNameError, SeriesSource,
    };

    fn add_unnamed(store: &SeriesStore) -> SeriesId {
        store
            .add_series(NewSeries::unnamed_serial_command("read value"))
            .unwrap()
    }

    fn add_named(store: &SeriesStore, name: &str) -> SeriesId {
        store
            .add_series(NewSeries::named_serial_command("read value", name))
            .unwrap()
    }

    #[test]
    fn assigns_unique_ids() {
        let store = SeriesStore::new();

        let first_id = add_unnamed(&store);
        let second_id = add_unnamed(&store);

        assert_ne!(first_id, second_id);

        let stored_ids =
            store.with(|series| series.iter().map(|series| series.id).collect::<Vec<_>>());

        assert_eq!(stored_ids, vec![first_id, second_id],);
    }

    #[test]
    fn does_not_reuse_ids_after_clear() {
        let store = SeriesStore::new();

        let first_id = add_unnamed(&store);

        store.clear();

        let second_id = add_unnamed(&store);

        assert_ne!(first_id, second_id);
    }

    #[test]
    fn changes_visibility_by_id() {
        let store = SeriesStore::new();

        let id = add_unnamed(&store);

        assert!(store.set_visibility(id, false));

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].id, id);
        assert!(!metadata[0].visible);
    }

    #[test]
    fn removes_series_by_id() {
        let store = SeriesStore::new();

        let first_id = add_unnamed(&store);
        let second_id = add_unnamed(&store);

        assert!(store.remove_series(first_id));

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].id, second_id);
    }

    #[test]
    fn reports_missing_series() {
        let store = SeriesStore::new();

        let id = add_unnamed(&store);

        assert!(store.remove_series(id));
        assert!(!store.remove_series(id));

        assert!(!store.set_visibility(id, false));
    }

    #[test]
    fn generates_unique_default_names() {
        let store = SeriesStore::new();

        add_unnamed(&store);
        add_unnamed(&store);

        let names = store.with(|series| {
            series
                .iter()
                .map(|series| series.name.clone())
                .collect::<Vec<_>>()
        });

        assert_eq!(names, vec!["serial1", "serial2"],);
    }

    #[test]
    fn does_not_reuse_default_names_after_clear() {
        let store = SeriesStore::new();

        add_unnamed(&store);

        store.clear();

        add_unnamed(&store);

        let names = store.with(|series| {
            series
                .iter()
                .map(|series| series.name.clone())
                .collect::<Vec<_>>()
        });

        assert_eq!(names, vec!["serial2"]);
    }

    #[test]
    fn accepts_custom_name() {
        let store = SeriesStore::new();

        add_named(&store, "temperature");

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);

        assert_eq!(metadata[0].name, "temperature",);
    }

    #[test]
    fn trims_custom_name() {
        let store = SeriesStore::new();

        add_named(&store, "  temperature  ");

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);

        assert_eq!(metadata[0].name, "temperature",);
    }

    #[test]
    fn rejects_duplicate_name() {
        let store = SeriesStore::new();

        add_named(&store, "temperature");

        let result = store.add_series(NewSeries::named_serial_command(
            "read another value",
            "temperature",
        ));

        assert_eq!(
            result,
            Err(AddSeriesError::InvalidName(SeriesNameError::Duplicate(
                "temperature".to_owned(),
            ),),),
        );
    }

    #[test]
    fn rejects_empty_name() {
        let store = SeriesStore::new();

        let result = store.add_series(NewSeries::named_serial_command("read value", "   "));

        assert_eq!(
            result,
            Err(AddSeriesError::InvalidName(SeriesNameError::Empty,),),
        );
    }

    #[test]
    fn rejects_name_with_whitespace() {
        let store = SeriesStore::new();

        let result = store.add_series(NewSeries::named_serial_command(
            "read value",
            "room temperature",
        ));

        assert_eq!(
            result,
            Err(AddSeriesError::InvalidName(
                SeriesNameError::ContainsWhitespace,
            ),),
        );
    }

    #[test]
    fn removes_series_by_name() {
        let store = SeriesStore::new();

        let id = add_named(&store, "temperature");

        assert_eq!(store.remove_series_by_name("temperature",), Some(id),);

        assert!(store.metadata().is_empty());

        assert_eq!(store.remove_series_by_name("temperature",), None,);
    }

    #[test]
    fn renames_series_without_changing_id() {
        let store = SeriesStore::new();

        let id = add_named(&store, "temperature");

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

        let id = add_named(&store, "temperature");

        assert_eq!(store.rename_series("temperature", "temperature",), Ok(id),);
    }

    #[test]
    fn rejects_duplicate_name_during_rename() {
        let store = SeriesStore::new();

        add_named(&store, "first");
        add_named(&store, "second");

        let result = store.rename_series("first", "second");

        assert_eq!(
            result,
            Err(RenameSeriesError::InvalidName(SeriesNameError::Duplicate(
                "second".to_owned(),
            ),),),
        );
    }

    #[test]
    fn reports_missing_series_during_rename() {
        let store = SeriesStore::new();

        let result = store.rename_series("missing", "new_name");

        assert_eq!(
            result,
            Err(RenameSeriesError::NotFound("missing".to_owned(),),),
        );
    }

    #[test]
    fn stores_serial_command_series() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::named_serial_command(
                "  read temperature  ",
                "temperature",
            ))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);

        assert_eq!(metadata[0].name, "temperature",);

        assert_eq!(
            metadata[0].source,
            SeriesSource::SerialCommand {
                command: "read temperature".to_owned(),
            },
        );
    }

    #[test]
    fn rejects_empty_serial_command() {
        let store = SeriesStore::new();

        let result = store.add_series(NewSeries::named_serial_command("   ", "temperature"));

        assert_eq!(result, Err(AddSeriesError::EmptySerialCommand,),);
    }

    #[test]
    fn rejects_serial_command_with_line_break() {
        let store = SeriesStore::new();

        let result = store.add_series(NewSeries::named_serial_command(
            "read value\nnext",
            "temperature",
        ));

        assert_eq!(result, Err(AddSeriesError::SerialCommandContainsLineBreak,),);
    }

    #[test]
    fn generates_name_for_serial_command() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::unnamed_serial_command("read value"))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);

        assert_eq!(metadata[0].name, "serial1",);

        assert_eq!(
            metadata[0].source,
            SeriesSource::SerialCommand {
                command: "read value".to_owned(),
            },
        );
    }

    #[test]
    fn stores_metakon_series() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::named_typed_metakon(
                1,
                0,
                0x01,
                MetakonValueType::Int,
                0.1,
                "temperature",
            ))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);

        assert_eq!(metadata[0].name, "temperature",);

        assert_eq!(
            metadata[0].source,
            SeriesSource::Metakon {
                device: 1,
                channel: 0,
                value_type: MetakonValueType::Int,
                register: 0x01,
                scale: 0.1,
            },
        );
    }

    #[test]
    fn generates_name_for_metakon_series() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::unnamed_typed_metakon(
                1,
                0,
                0x01,
                MetakonValueType::Int,
                0.1,
            ))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);

        assert_eq!(metadata[0].name, "metakon1",);
    }

    #[test]
    fn rejects_invalid_metakon_scale() {
        let store = SeriesStore::new();

        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let result = store.add_series(NewSeries::unnamed_typed_metakon(
                1,
                0,
                0x01,
                MetakonValueType::Int,
                scale,
            ));

            assert_eq!(result, Err(AddSeriesError::InvalidMetakonScale,),);
        }
    }
}
