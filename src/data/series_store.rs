use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

use crate::{connection::ConnectionId, instrument::InstrumentReadRequest};

use super::{
    NewSeries, Series, SeriesColor, SeriesId, SeriesMetadata, SeriesNameError, SeriesPollingState,
    SeriesSample, SeriesSource, series_name::normalize_series_name,
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
    InvalidInstrumentScale,
}

impl std::fmt::Display for AddSeriesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidName(error) => error.fmt(formatter),

            Self::EmptySerialCommand => formatter.write_str("Serial command cannot be empty"),

            Self::SerialCommandContainsLineBreak => {
                formatter.write_str("Serial command cannot contain a line break")
            }

            Self::InvalidInstrumentScale => formatter.write_str(
                "Instrument series scale must be finite \
                     and greater than zero",
            ),
        }
    }
}

impl std::error::Error for AddSeriesError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidName(error) => Some(error),

            Self::EmptySerialCommand
            | Self::SerialCommandContainsLineBreak
            | Self::InvalidInstrumentScale => None,
        }
    }
}

impl From<SeriesNameError> for AddSeriesError {
    fn from(error: SeriesNameError) -> Self {
        Self::InvalidName(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppendSeriesSamplesError {
    UnknownSeries(SeriesId),
}

impl std::fmt::Display for AppendSeriesSamplesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownSeries(id) => {
                write!(formatter, "Cannot append sample for unknown series {id}",)
            }
        }
    }
}

impl std::error::Error for AppendSeriesSamplesError {}

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
        let connection_id = new_series.connection_id();
        let color = new_series.color();

        let (source, requested_name, sampling_interval) = new_series.into_parts();

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

            series.push(Series::new(
                id,
                name,
                source,
                sampling_interval,
                connection_id,
                color,
            ));
            Ok(id)
        })
    }

    pub fn append_samples(&self, samples: &[SeriesSample]) -> Result<(), AppendSeriesSamplesError> {
        self.with_mut(|series| {
            for series_sample in samples {
                if !series
                    .iter()
                    .any(|series| series.id == series_sample.series_id)
                {
                    return Err(AppendSeriesSamplesError::UnknownSeries(
                        series_sample.series_id,
                    ));
                }
            }

            for series_sample in samples {
                let target = series
                    .iter_mut()
                    .find(|series| series.id == series_sample.series_id)
                    .expect("series IDs were validated before appending samples");

                target.samples.push(series_sample.sample);
            }

            Ok(())
        })
    }

    pub fn id_by_name(&self, name: &str) -> Option<SeriesId> {
        self.with(|series| {
            series
                .iter()
                .find(|series| series.name == name)
                .map(|series| series.id)
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

    pub fn polling_metadata_for_connection(
        &self,
        connection_id: ConnectionId,
    ) -> Vec<SeriesMetadata> {
        self.with(|series| {
            series
                .iter()
                .filter(|series| {
                    series.connection_id == connection_id
                        && series.polling_state == SeriesPollingState::Enabled
                })
                .map(SeriesMetadata::from)
                .collect()
        })
    }

    pub fn suspend_polling(&self, id: SeriesId) -> bool {
        self.with_mut(|series| {
            let Some(series) = series.iter_mut().find(|series| series.id == id) else {
                return false;
            };

            if series.polling_state == SeriesPollingState::Suspended {
                return false;
            }

            series.polling_state = SeriesPollingState::Suspended;

            true
        })
    }

    pub fn resume_instrument_polling(
        &self,
        connection_id: ConnectionId,
        request: InstrumentReadRequest,
    ) -> Vec<(SeriesId, String)> {
        self.with_mut(|series| {
            let mut resumed = Vec::new();

            for series in series.iter_mut().filter(|series| {
                series.connection_id == connection_id
                    && series.polling_state == SeriesPollingState::Suspended
            }) {
                let SeriesSource::Instrument(stored_request) = &series.source else {
                    continue;
                };

                if !stored_request.refers_to_same_parameter(&request) {
                    continue;
                }

                series.polling_state = SeriesPollingState::Enabled;

                resumed.push((series.id, series.name.clone()));
            }

            resumed
        })
    }

    pub fn resume_polling_for_connection(&self, connection_id: ConnectionId) -> usize {
        self.with_mut(|series| {
            let mut resumed = 0;

            for series in series.iter_mut().filter(|series| {
                series.connection_id == connection_id
                    && series.polling_state == SeriesPollingState::Suspended
            }) {
                series.polling_state = SeriesPollingState::Enabled;

                resumed += 1;
            }

            resumed
        })
    }

    pub fn resume_polling_by_name(&self, name: &str) -> Option<(SeriesId, ConnectionId, bool)> {
        self.with_mut(|series| {
            let series = series.iter_mut().find(|series| series.name == name)?;

            let was_suspended = series.polling_state == SeriesPollingState::Suspended;

            series.polling_state = SeriesPollingState::Enabled;

            Some((series.id, series.connection_id, was_suspended))
        })
    }

    pub fn resume_all_polling(&self) -> Vec<(SeriesId, String, ConnectionId)> {
        self.with_mut(|series| {
            series
                .iter_mut()
                .filter_map(|series| {
                    if series.polling_state != SeriesPollingState::Suspended {
                        return None;
                    }

                    series.polling_state = SeriesPollingState::Enabled;

                    Some((series.id, series.name.clone(), series.connection_id))
                })
                .collect()
        })
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

    pub fn set_color_by_name(&self, name: &str, color: Option<SeriesColor>) -> Option<SeriesId> {
        self.with_mut(|series| {
            let series = series.iter_mut().find(|series| series.name == name)?;

            series.color = color;

            Some(series.id)
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

        SeriesSource::Instrument(request) => {
            let scale = request.scale();

            if !scale.is_finite() || scale <= 0.0 {
                return Err(AddSeriesError::InvalidInstrumentScale);
            }

            Ok(SeriesSource::Instrument(request))
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
    use super::{AppendSeriesSamplesError, RenameSeriesError, SeriesStore};

    use crate::{
        connection::ConnectionId,
        data::{
            AddSeriesError, NewSeries, Sample, SamplingInterval, SeriesColor, SeriesId,
            SeriesNameError, SeriesPollingState, SeriesSample, SeriesSource,
        },
        instrument::{
            InstrumentReadRequest,
            metakon_5x3::{Metakon5x3, Metakon5x3Register},
            virtual_instrument::{VirtualInstrumentId, VirtualParameterId},
        },
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

        assert_eq!(store.id_by_name("temperature"), Some(id),);

        assert!(store.remove_series(id));

        assert_eq!(store.id_by_name("temperature"), None,);

        assert!(!store.remove_series(id));
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

        let request = InstrumentReadRequest::metakon_5x3(
            Metakon5x3::new(1, 0),
            Metakon5x3Register::Measurement,
            0.1,
        );

        store
            .add_series(NewSeries::named_instrument(request, "temperature"))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].name, "temperature");

        assert_eq!(metadata[0].source, SeriesSource::Instrument(request),);
    }

    #[test]
    fn generates_name_for_metakon_series() {
        let store = SeriesStore::new();

        let request = InstrumentReadRequest::metakon_5x3(
            Metakon5x3::new(1, 0),
            Metakon5x3Register::Measurement,
            0.1,
        );

        store
            .add_series(NewSeries::unnamed_instrument(request))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].name, "metakon1");
    }

    #[test]
    fn rejects_invalid_instrument_scale() {
        let store = SeriesStore::new();

        for scale in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let request = InstrumentReadRequest::metakon_5x3(
                Metakon5x3::new(1, 0),
                Metakon5x3Register::Measurement,
                scale,
            );

            let result = store.add_series(NewSeries::unnamed_instrument(request));

            assert_eq!(result, Err(AddSeriesError::InvalidInstrumentScale,),);
        }
    }

    #[test]
    fn stores_virtual_instrument_series() {
        let store = SeriesStore::new();

        let request = InstrumentReadRequest::virtual_instrument(
            VirtualInstrumentId::new(1),
            VirtualParameterId::new(2),
        );

        store
            .add_series(NewSeries::unnamed_instrument(request))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);
        assert_eq!(metadata[0].name, "virtual1");

        assert_eq!(metadata[0].source, SeriesSource::Instrument(request),);
    }

    #[test]
    fn stores_custom_sampling_interval() {
        let store = SeriesStore::new();

        let interval = SamplingInterval::from_secs_f64(2.5).unwrap();

        store
            .add_series(
                NewSeries::named_serial_command("read value", "temperature")
                    .with_sampling_interval(interval),
            )
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata.len(), 1);

        assert_eq!(metadata[0].sampling_interval, Some(interval),);
    }

    #[test]
    fn leaves_sampling_interval_unspecified_by_default() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::unnamed_serial_command("read value"))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata[0].sampling_interval, None);
    }

    #[test]
    fn uses_primary_connection_by_default() {
        let store = SeriesStore::new();

        store
            .add_series(NewSeries::unnamed_serial_command("read value"))
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata[0].connection_id, ConnectionId::PRIMARY,);
    }

    #[test]
    fn stores_custom_connection() {
        let store = SeriesStore::new();

        let connection_id = ConnectionId::new(7);

        store
            .add_series(
                NewSeries::unnamed_serial_command("read value").with_connection(connection_id),
            )
            .unwrap();

        let metadata = store.metadata();

        assert_eq!(metadata[0].connection_id, connection_id,);
    }

    #[test]
    fn returns_metadata_for_requested_connection() {
        let store = SeriesStore::new();

        let primary_id = store
            .add_series(
                NewSeries::named_serial_command("read primary", "primary")
                    .with_connection(ConnectionId::PRIMARY),
            )
            .unwrap();

        let secondary_connection = ConnectionId::new(2);

        let secondary_id = store
            .add_series(
                NewSeries::named_serial_command("read secondary", "secondary")
                    .with_connection(secondary_connection),
            )
            .unwrap();

        let primary = store.polling_metadata_for_connection(ConnectionId::PRIMARY);

        assert_eq!(primary.len(), 1);
        assert_eq!(primary[0].id, primary_id);

        let secondary = store.polling_metadata_for_connection(secondary_connection);

        assert_eq!(secondary.len(), 1);
        assert_eq!(secondary[0].id, secondary_id);
    }

    #[test]
    fn suspends_and_resumes_series_polling() {
        let store = SeriesStore::new();

        let id = add_named(&store, "temperature");

        assert!(store.suspend_polling(id));
        assert!(!store.suspend_polling(id));

        let state = store.with(|series| {
            series
                .iter()
                .find(|series| series.id == id)
                .unwrap()
                .polling_state
        });

        assert_eq!(state, SeriesPollingState::Suspended,);

        assert!(
            store
                .polling_metadata_for_connection(ConnectionId::PRIMARY,)
                .is_empty()
        );

        assert_eq!(
            store.resume_polling_for_connection(ConnectionId::PRIMARY,),
            1,
        );

        let state = store.with(|series| {
            series
                .iter()
                .find(|series| series.id == id)
                .unwrap()
                .polling_state
        });

        assert_eq!(state, SeriesPollingState::Enabled,);

        assert_eq!(
            store
                .polling_metadata_for_connection(ConnectionId::PRIMARY,)
                .len(),
            1,
        );
    }

    #[test]
    fn resumes_matching_instrument_series() {
        let store = SeriesStore::new();

        let instrument = Metakon5x3::new(15, 0);

        let measurement_request =
            InstrumentReadRequest::metakon_5x3(instrument, Metakon5x3Register::Measurement, 1.0);

        let setpoint_request =
            InstrumentReadRequest::metakon_5x3(instrument, Metakon5x3Register::Setpoint, 1.0);

        let measurement_id = store
            .add_series(NewSeries::named_instrument(
                measurement_request,
                "temperature",
            ))
            .unwrap();

        let setpoint_id = store
            .add_series(NewSeries::named_instrument(setpoint_request, "setpoint"))
            .unwrap();

        assert!(store.suspend_polling(measurement_id));

        assert!(store.suspend_polling(setpoint_id));

        let resumed = store.resume_instrument_polling(ConnectionId::PRIMARY, measurement_request);

        assert_eq!(resumed, vec![(measurement_id, "temperature".to_owned(),)],);

        let series = store.with(|series| {
            series
                .iter()
                .map(|series| (series.id, series.polling_state))
                .collect::<Vec<_>>()
        });

        assert_eq!(
            series,
            vec![
                (measurement_id, SeriesPollingState::Enabled,),
                (setpoint_id, SeriesPollingState::Suspended,),
            ],
        );
    }

    #[test]
    fn resumes_series_polling_by_name() {
        let store = SeriesStore::new();

        let id = add_named(&store, "temperature");

        assert!(store.suspend_polling(id));

        assert_eq!(
            store.resume_polling_by_name("temperature"),
            Some((id, ConnectionId::PRIMARY, true,)),
        );

        assert_eq!(
            store.metadata()[0].polling_state,
            SeriesPollingState::Enabled,
        );

        assert_eq!(
            store.resume_polling_by_name("temperature"),
            Some((id, ConnectionId::PRIMARY, false,)),
        );

        assert_eq!(store.resume_polling_by_name("missing"), None,);
    }

    #[test]
    fn resumes_all_suspended_series() {
        let store = SeriesStore::new();

        let primary_id = store
            .add_series(
                NewSeries::named_serial_command("read primary", "primary")
                    .with_connection(ConnectionId::PRIMARY),
            )
            .unwrap();

        let secondary_connection = ConnectionId::new(2);

        let secondary_id = store
            .add_series(
                NewSeries::named_serial_command("read secondary", "secondary")
                    .with_connection(secondary_connection),
            )
            .unwrap();

        let enabled_id = add_named(&store, "enabled");

        assert!(store.suspend_polling(primary_id));
        assert!(store.suspend_polling(secondary_id));

        assert_eq!(
            store.resume_all_polling(),
            vec![
                (primary_id, "primary".to_owned(), ConnectionId::PRIMARY,),
                (secondary_id, "secondary".to_owned(), secondary_connection,),
            ],
        );

        let metadata = store.metadata();

        assert!(
            metadata
                .iter()
                .all(|series| { series.polling_state == SeriesPollingState::Enabled })
        );

        assert!(metadata.iter().any(|series| { series.id == enabled_id }));

        assert!(store.resume_all_polling().is_empty());
    }

    #[test]
    fn stores_explicit_series_color() {
        let store = SeriesStore::new();
        let color = SeriesColor::new(0x1A, 0x2B, 0x3C);

        store
            .add_series(
                NewSeries::named_serial_command("read temperature", "temperature")
                    .with_color(color),
            )
            .unwrap();

        store.with(|series| {
            assert_eq!(series.len(), 1);
            assert_eq!(series[0].color, Some(color));
        });
    }

    #[test]
    fn changes_and_resets_series_color_by_name() {
        let store = SeriesStore::new();
        let color = SeriesColor::new(0x1A, 0x2B, 0x3C);

        let id = store
            .add_series(NewSeries::named_serial_command(
                "read temperature",
                "temperature",
            ))
            .unwrap();

        assert_eq!(
            store.set_color_by_name("temperature", Some(color),),
            Some(id),
        );

        store.with(|series| {
            assert_eq!(series[0].color, Some(color));
        });

        assert_eq!(store.set_color_by_name("temperature", None), Some(id),);

        store.with(|series| {
            assert_eq!(series[0].color, None);
        });

        assert_eq!(store.set_color_by_name("missing", Some(color),), None,);
    }

    #[test]
    fn appends_samples_atomically() {
        let store = SeriesStore::new();

        let id = add_named(&store, "temperature");

        let valid = SeriesSample::new(id, Sample::new(1.0, 100.0));

        let missing_id = SeriesId::new(999);

        let invalid = SeriesSample::new(missing_id, Sample::new(1.0, 200.0));

        assert_eq!(
            store.append_samples(&[valid, invalid]),
            Err(AppendSeriesSamplesError::UnknownSeries(missing_id,)),
        );

        store.with(|series| {
            assert!(series[0].samples.is_empty());
        });

        assert_eq!(store.append_samples(&[valid]), Ok(()));

        store.with(|series| {
            assert_eq!(series[0].samples, vec![Sample::new(1.0, 100.0)],);
        });
    }
}
