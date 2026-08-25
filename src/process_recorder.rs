use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::{self, JoinHandle},
    time::SystemTime,
};

mod sqlite;

pub(crate) use sqlite::{SqliteProcessRecordWriter, new_process_database_path};

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::{
    connection::ConnectionId,
    data::{SeriesId, SeriesMetadata, SeriesSample},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessLogLevel {
    Info,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessActionOrigin {
    UserInterface,
    Lua,
    ProcessControl,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ProcessAction {
    StartAcquisition,
    StopAcquisition,
    ClearSeries,
    StartEmulator,
    StopEmulator,

    AddSeries {
        connection_id: ConnectionId,
        name: Option<String>,
        source: String,
        polling_interval_seconds: Option<f64>,
        color: Option<String>,
    },

    AddFilteredSeries {
        input_name: String,
        name: String,
        definition: String,
        color: Option<String>,
    },

    AddPidLoop {
        connection_id: ConnectionId,
        name: String,
        input_name: String,
        output_target: String,
        setpoint: f64,
        proportional_gain: f64,
        integral_gain: f64,
        derivative_gain: f64,
        output_minimum: f64,
        output_maximum: f64,
    },

    SetPidSetpoint {
        name: String,
        setpoint: f64,
    },

    SetFilter {
        name: String,
        definition: String,
    },

    DeleteSeriesByName {
        name: String,
    },

    RenameSeries {
        current_name: String,
        new_name: String,
    },

    SetSeriesColor {
        name: String,
        color: Option<String>,
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
pub struct ProcessMeasurement {
    pub connection_id: ConnectionId,
    pub series_id: SeriesId,
    pub series_name: String,
    pub timestamp: f64,
    pub value: f64,
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

pub trait ProcessRecordWriter: Send {
    fn write(&mut self, record: ProcessRecord) -> Result<(), ProcessRecorderError>;

    fn flush(&mut self) -> Result<(), ProcessRecorderError> {
        Ok(())
    }
}

#[derive(Default)]
pub struct NullProcessRecordWriter;

impl ProcessRecordWriter for NullProcessRecordWriter {
    fn write(&mut self, _record: ProcessRecord) -> Result<(), ProcessRecorderError> {
        Ok(())
    }
}

trait ProcessRecordSink: Send + Sync {
    fn record(&self, record: ProcessRecord);

    fn take_error(&self) -> Option<String> {
        None
    }
}

#[derive(Default)]
struct DisabledProcessRecordSink;

impl ProcessRecordSink for DisabledProcessRecordSink {
    fn record(&self, _record: ProcessRecord) {}
}

enum ProcessRecorderCommand {
    Record(ProcessRecord),
    Shutdown,
}

struct AsyncProcessRecordSink {
    sender: Sender<ProcessRecorderCommand>,
    thread: Option<JoinHandle<()>>,
    error: Arc<Mutex<Option<String>>>,
}

impl AsyncProcessRecordSink {
    fn spawn(writer: impl ProcessRecordWriter + 'static) -> io::Result<Self> {
        let (sender, receiver) = unbounded();

        let error = Arc::new(Mutex::new(None));

        let thread_error = Arc::clone(&error);

        let thread = thread::Builder::new()
            .name("process-recorder".to_owned())
            .spawn(move || {
                run_process_recorder(receiver, Box::new(writer), thread_error);
            })?;

        Ok(Self {
            sender,
            thread: Some(thread),
            error,
        })
    }
}

impl ProcessRecordSink for AsyncProcessRecordSink {
    fn record(&self, record: ProcessRecord) {
        if self
            .sender
            .send(ProcessRecorderCommand::Record(record))
            .is_err()
        {
            store_first_error(&self.error, "Process recorder thread is disconnected");
        }
    }

    fn take_error(&self) -> Option<String> {
        self.error
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
    }
}

impl Drop for AsyncProcessRecordSink {
    fn drop(&mut self) {
        let _ = self.sender.send(ProcessRecorderCommand::Shutdown);

        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            store_first_error(&self.error, "Process recorder thread panicked");
        }
    }
}

fn run_process_recorder(
    receiver: Receiver<ProcessRecorderCommand>,
    mut writer: Box<dyn ProcessRecordWriter>,
    error: Arc<Mutex<Option<String>>>,
) {
    let mut writer_available = true;

    while let Ok(command) = receiver.recv() {
        match command {
            ProcessRecorderCommand::Record(record) if writer_available => {
                if let Err(write_error) = writer.write(record) {
                    store_first_error(&error, write_error.to_string());

                    writer_available = false;
                }
            }

            ProcessRecorderCommand::Record(_) => {}

            ProcessRecorderCommand::Shutdown => {
                break;
            }
        }
    }

    if writer_available && let Err(flush_error) = writer.flush() {
        store_first_error(&error, flush_error.to_string());
    }
}

fn store_first_error(destination: &Mutex<Option<String>>, error: impl Into<String>) {
    let mut stored = destination
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    if stored.is_none() {
        *stored = Some(error.into());
    }
}

#[derive(Clone)]
pub struct ProcessRecorder {
    sink: Arc<dyn ProcessRecordSink>,
}

impl ProcessRecorder {
    pub fn spawn(writer: impl ProcessRecordWriter + 'static) -> io::Result<Self> {
        Ok(Self {
            sink: Arc::new(AsyncProcessRecordSink::spawn(writer)?),
        })
    }

    pub fn record(&self, record: ProcessRecord) {
        self.sink.record(record);
    }

    pub fn record_action(&self, origin: ProcessActionOrigin, action: ProcessAction) {
        self.record(ProcessRecord::ActionRequested {
            timestamp: SystemTime::now(),
            origin,
            action,
        });
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
                        "successfully stored sample \
                         must have matching series \
                         metadata",
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

    pub fn take_error(&self) -> Option<String> {
        self.sink.take_error()
    }
}

impl Default for ProcessRecorder {
    fn default() -> Self {
        Self {
            sink: Arc::new(DisabledProcessRecordSink),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessRecorderError {
    message: String,
}

impl ProcessRecorderError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ProcessRecorderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ProcessRecorderError {}

impl From<String> for ProcessRecorderError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for ProcessRecorderError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<io::Error> for ProcessRecorderError {
    fn from(error: io::Error) -> Self {
        Self::new(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        ProcessLogLevel, ProcessRecord, ProcessRecordWriter, ProcessRecorder, ProcessRecorderError,
    };

    struct CollectingWriter {
        records: Arc<Mutex<Vec<ProcessRecord>>>,
    }

    impl ProcessRecordWriter for CollectingWriter {
        fn write(&mut self, record: ProcessRecord) -> Result<(), ProcessRecorderError> {
            self.records.lock().unwrap().push(record);

            Ok(())
        }
    }

    #[test]
    fn writes_records_on_background_thread() {
        let records = Arc::new(Mutex::new(Vec::new()));

        let recorder = ProcessRecorder::spawn(CollectingWriter {
            records: Arc::clone(&records),
        })
        .unwrap();

        recorder.record(ProcessRecord::Log {
            timestamp: std::time::SystemTime::now(),
            level: ProcessLogLevel::Info,
            message: "Application started".to_owned(),
        });

        drop(recorder);

        let records = records.lock().unwrap();

        assert_eq!(records.len(), 1);

        assert!(matches!(
            &records[0],
            ProcessRecord::Log {
                level: ProcessLogLevel::Info,
                message,
                ..
            } if message == "Application started",
        ));
    }
}
