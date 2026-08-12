use std::{path::PathBuf, sync::Arc, time::SystemTime};

use crate::{
    connection::ConnectionId,
    data::{SeriesId, SeriesMetadata, SeriesSample},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessLogLevel {
    Info,
    Error,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProcessMeasurement {
    pub connection_id: ConnectionId,
    pub series_id: SeriesId,
    pub series_name: String,
    pub timestamp: f64,
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessActionOrigin {
    UserInterface,
    Lua,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessAction {
    StartAcquisition,
    StopAcquisition,
    ClearSeries,
    StartRecording,
    StopRecording,
    StartEmulator,
    StopEmulator,

    AddSeries {
        connection_id: ConnectionId,
        name: Option<String>,
        source: String,
        polling_interval_seconds: Option<f64>,
    },

    DeleteSeriesByName {
        name: String,
    },

    RemoveSeries {
        series_id: SeriesId,
    },

    RenameSeries {
        current_name: String,
        new_name: String,
    },

    SetSeriesVisibility {
        series_id: SeriesId,
        visible: bool,
    },

    SendSerial {
        connection_id: ConnectionId,
        command: String,
    },

    ReadInstrument {
        connection_id: ConnectionId,
        request: String,
    },

    WriteInstrument {
        connection_id: ConnectionId,
        request: String,
    },

    DescribeVirtualInstruments {
        connection_id: ConnectionId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessRecord {
    Log {
        timestamp: SystemTime,
        level: ProcessLogLevel,
        message: String,
    },

    ConfigurationLoaded {
        timestamp: SystemTime,
        startup_path: PathBuf,
        source: Option<String>,
    },

    Measurements {
        measurements: Vec<ProcessMeasurement>,
    },

    ActionRequested {
        timestamp: SystemTime,
        origin: ProcessActionOrigin,
        action: ProcessAction,
    },
}

pub trait ProcessRecordSink: Send + Sync {
    fn record(&self, record: ProcessRecord);
}

#[derive(Default)]
struct DisabledProcessRecordSink;

impl ProcessRecordSink for DisabledProcessRecordSink {
    fn record(&self, _record: ProcessRecord) {}
}

#[derive(Clone)]
pub struct ProcessRecorder {
    sink: Arc<dyn ProcessRecordSink>,
}

impl ProcessRecorder {
    pub fn new(sink: impl ProcessRecordSink + 'static) -> Self {
        Self {
            sink: Arc::new(sink),
        }
    }

    pub fn record(&self, record: ProcessRecord) {
        self.sink.record(record);
    }

    pub fn record_measurements(
        &self,
        connection_id: ConnectionId,
        samples: &[SeriesSample],
        series: &[SeriesMetadata],
    ) {
        if samples.is_empty() {
            return;
        }

        let measurements = samples
            .iter()
            .map(|series_sample| {
                let metadata = series
                    .iter()
                    .find(|metadata| metadata.id == series_sample.series_id)
                    .expect(
                        "successfully stored sample must \
                         have matching series metadata",
                    );

                ProcessMeasurement {
                    connection_id,
                    series_id: series_sample.series_id,
                    series_name: metadata.name.clone(),
                    timestamp: series_sample.sample.timestamp,
                    value: series_sample.sample.value,
                }
            })
            .collect();

        self.record(ProcessRecord::Measurements { measurements });
    }

    pub fn record_action(&self, origin: ProcessActionOrigin, action: ProcessAction) {
        self.record(ProcessRecord::ActionRequested {
            timestamp: SystemTime::now(),
            origin,
            action,
        });
    }
}

impl Default for ProcessRecorder {
    fn default() -> Self {
        Self::new(DisabledProcessRecordSink)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::SystemTime,
    };

    use super::{ProcessLogLevel, ProcessRecord, ProcessRecordSink, ProcessRecorder};

    struct CollectingSink {
        records: Arc<Mutex<Vec<ProcessRecord>>>,
    }

    impl ProcessRecordSink for CollectingSink {
        fn record(&self, record: ProcessRecord) {
            self.records.lock().unwrap().push(record);
        }
    }

    #[test]
    fn forwards_records_to_sink() {
        let records = Arc::new(Mutex::new(Vec::new()));

        let recorder = ProcessRecorder::new(CollectingSink {
            records: Arc::clone(&records),
        });

        let timestamp = SystemTime::now();

        recorder.record(ProcessRecord::Log {
            timestamp,
            level: ProcessLogLevel::Info,
            message: "Application started".to_owned(),
        });

        assert_eq!(
            *records.lock().unwrap(),
            vec![ProcessRecord::Log {
                timestamp,
                level: ProcessLogLevel::Info,
                message: "Application started".to_owned(),
            }],
        );
    }
}
