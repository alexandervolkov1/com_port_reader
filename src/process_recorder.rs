//! Asynchronous process-history recording.
//!
//! Measurements, user and Lua actions, and logs are sent to a dedicated writer thread. Recorder
//! failures are surfaced as application events but deliberately do not stop acquisition or control.

use std::{
    io,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::SystemTime,
};

use crossbeam_channel::{Receiver, Sender, unbounded};

use crate::{
    connection::ConnectionId,
    data::{SeriesId, SeriesMetadata, SeriesSample},
    instrument::InstrumentValue,
    process_control::{ControllerKind, ReferenceSource},
};

mod sqlite;

pub(crate) use sqlite::{SqliteProcessRecordWriter, new_process_database_path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Severity attached to a process log record.
pub enum ProcessLogLevel {
    Info,
    Error,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessActionOrigin {
    UserInterface,
    Lua,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ProcessActionId(u64);

impl ProcessActionId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProcessActionContext {
    action_id: ProcessActionId,
}

impl ProcessActionContext {
    pub const fn new(action_id: ProcessActionId) -> Self {
        Self { action_id }
    }

    pub const fn action_id(self) -> ProcessActionId {
        self.action_id
    }
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

    AddController {
        connection_id: ConnectionId,
        name: String,
        input_name: String,
        output_target: String,
        kind: ControllerKind,
        parameters: Vec<(String, InstrumentValue)>,
    },

    WriteControllerParameter {
        name: String,
        key: String,
        value: InstrumentValue,
    },

    ConfigureController {
        name: String,
        updates: Vec<(String, InstrumentValue)>,
    },

    WriteControllerReferenceParameter {
        name: String,
        key: String,
        value: InstrumentValue,
    },

    ConfigureControllerReference {
        name: String,
        updates: Vec<(String, InstrumentValue)>,
    },

    SetControllerReference {
        name: String,
        source: ReferenceSource,
    },

    SetControllerInput {
        name: String,
        input_name: String,
    },

    PauseController {
        name: String,
    },

    RemoveController {
        name: String,
    },

    ResumeController {
        name: String,
    },

    ResetControllerIntegral {
        name: String,
    },

    ResetController {
        name: String,
    },

    SetFilter {
        series_id: Option<SeriesId>,
        name: String,
        definition: String,
    },

    DeleteSeriesByName {
        series_id: Option<SeriesId>,
        name: String,
    },

    RenameSeries {
        series_id: Option<SeriesId>,
        current_name: String,
        new_name: String,
    },

    SetSeriesColor {
        series_id: Option<SeriesId>,
        name: String,
        color: Option<String>,
    },

    SetSeriesVisibility {
        series_id: SeriesId,
        series_name: Option<String>,
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
pub enum ProcessActionResult {
    SerialResponse(String),
    InstrumentValue(InstrumentValue),
    VirtualInstrumentCount(usize),
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
pub struct ProcessControlOutput {
    pub timestamp: f64,
    pub loop_name: String,
    pub controller_kind: String,
    pub input_series_id: SeriesId,
    pub connection_id: ConnectionId,
    pub setpoint: Option<f64>,
    pub measurement: f64,
    pub requested_output: f64,
    pub actual_output: Option<f64>,
    pub unconstrained_output: Option<f64>,
    pub proportional: Option<f64>,
    pub integral: Option<f64>,
    pub derivative: Option<f64>,
    pub saturated: Option<bool>,
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
    ControlOutput {
        output: ProcessControlOutput,
    },
    ActionRequested {
        action_id: ProcessActionId,
        timestamp: SystemTime,
        origin: ProcessActionOrigin,
        action: ProcessAction,
    },

    ActionApplied {
        action_id: ProcessActionId,
        timestamp: SystemTime,
        series_id: Option<SeriesId>,
        series_name: Option<String>,
        result: Option<ProcessActionResult>,
    },

    ActionFailed {
        action_id: ProcessActionId,
        timestamp: SystemTime,
        error: String,
    },
}

impl ProcessRecord {
    fn is_timeline_event(&self) -> bool {
        matches!(
            self,
            Self::Log { .. }
                | Self::ConfigurationLoaded { .. }
                | Self::ActionRequested { .. }
                | Self::ActionApplied { .. }
                | Self::ActionFailed { .. }
        )
    }
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
    next_action_id: Arc<AtomicU64>,
    timeline_senders: Arc<Mutex<Vec<Sender<ProcessRecord>>>>,
    application_events: crate::application_event::ApplicationEventHub,
}

impl ProcessRecorder {
    pub fn spawn_with_events(
        writer: impl ProcessRecordWriter + 'static,
        application_events: crate::application_event::ApplicationEventHub,
    ) -> io::Result<Self> {
        Ok(Self {
            sink: Arc::new(AsyncProcessRecordSink::spawn(writer)?),
            next_action_id: Arc::new(AtomicU64::new(1)),
            timeline_senders: Arc::new(Mutex::new(Vec::new())),
            application_events,
        })
    }

    pub(crate) fn application_events(&self) -> crate::application_event::ApplicationEventHub {
        self.application_events.clone()
    }

    pub(crate) fn subscribe_timeline(&self) -> Receiver<ProcessRecord> {
        let (sender, receiver) = unbounded();

        self.timeline_senders
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(sender);

        receiver
    }

    pub fn record(&self, record: ProcessRecord) {
        if let Some(event) =
            crate::application_event::ApplicationEvent::from_process_record(&record)
        {
            self.application_events.publish(event);
        }

        if record.is_timeline_event() {
            let mut senders = self
                .timeline_senders
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());

            senders.retain(|sender| sender.send(record.clone()).is_ok());
        }

        self.sink.record(record);
    }

    pub fn record_action(
        &self,
        origin: ProcessActionOrigin,
        action: ProcessAction,
    ) -> ProcessActionId {
        let action_id = ProcessActionId::new(self.next_action_id.fetch_add(1, Ordering::Relaxed));

        self.record(ProcessRecord::ActionRequested {
            action_id,
            timestamp: SystemTime::now(),
            origin,
            action,
        });

        action_id
    }

    pub fn record_action_applied(
        &self,
        action_id: ProcessActionId,
        series_id: Option<SeriesId>,
        series_name: Option<String>,
    ) {
        self.record(ProcessRecord::ActionApplied {
            action_id,
            timestamp: SystemTime::now(),
            series_id,
            series_name,
            result: None,
        });
    }

    pub fn record_action_applied_with_result(
        &self,
        action_id: ProcessActionId,
        result: ProcessActionResult,
    ) {
        self.record(ProcessRecord::ActionApplied {
            action_id,
            timestamp: SystemTime::now(),
            series_id: None,
            series_name: None,
            result: Some(result),
        });
    }

    pub fn record_action_failed(&self, action_id: ProcessActionId, error: impl Into<String>) {
        self.record(ProcessRecord::ActionFailed {
            action_id,
            timestamp: SystemTime::now(),
            error: error.into(),
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

    pub fn record_control_output(&self, output: ProcessControlOutput) {
        self.record(ProcessRecord::ControlOutput { output });
    }

    pub fn take_error(&self) -> Option<String> {
        self.sink.take_error()
    }
}

impl Default for ProcessRecorder {
    fn default() -> Self {
        Self {
            sink: Arc::new(DisabledProcessRecordSink),
            next_action_id: Arc::new(AtomicU64::new(1)),
            timeline_senders: Arc::new(Mutex::new(Vec::new())),
            application_events: crate::application_event::ApplicationEventHub::new(),
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
        ProcessAction, ProcessActionOrigin, ProcessActionResult, ProcessLogLevel, ProcessRecord,
        ProcessRecordWriter, ProcessRecorder, ProcessRecorderError,
    };
    use crate::{application_event::ApplicationEvent, connection::ConnectionId, data::SeriesId};

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

        let recorder = ProcessRecorder::spawn_with_events(
            CollectingWriter {
                records: Arc::clone(&records),
            },
            crate::application_event::ApplicationEventHub::new(),
        )
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

    #[test]
    fn publishes_timeline_records_to_subscribers() {
        let recorder = ProcessRecorder::default();

        let receiver = recorder.subscribe_timeline();

        recorder.record(ProcessRecord::Log {
            timestamp: std::time::SystemTime::now(),
            level: ProcessLogLevel::Info,
            message: "Test timeline event".to_owned(),
        });

        let record = receiver.try_recv().unwrap();

        assert!(matches!(
            record,
            ProcessRecord::Log {
                level: ProcessLogLevel::Info,
                message,
                ..
            } if message == "Test timeline event"
        ));
    }

    #[test]
    fn publishes_action_result_to_timeline() {
        let recorder = ProcessRecorder::default();
        let receiver = recorder.subscribe_timeline();

        let action_id = recorder.record_action(
            ProcessActionOrigin::UserInterface,
            ProcessAction::SendSerial {
                connection_id: ConnectionId::PRIMARY,
                command: "get".to_owned(),
            },
        );

        recorder.record_action_applied_with_result(
            action_id,
            ProcessActionResult::SerialResponse("42".to_owned()),
        );

        let _requested = receiver.try_recv().unwrap();

        let applied = receiver.try_recv().unwrap();

        assert!(matches!(
            applied,
            ProcessRecord::ActionApplied {
                result: Some(
                    ProcessActionResult::SerialResponse(
                        response
                    )
                ),
                ..
            } if response == "42"
        ));
    }

    #[test]
    fn publishes_action_lifecycle_to_application_events() {
        let events = crate::application_event::ApplicationEventHub::new();
        let receiver = events.subscribe();
        let recorder =
            ProcessRecorder::spawn_with_events(super::NullProcessRecordWriter, events).unwrap();
        let action = ProcessAction::SendSerial {
            connection_id: ConnectionId::PRIMARY,
            command: "get".to_owned(),
        };

        let action_id = recorder.record_action(ProcessActionOrigin::Lua, action.clone());
        recorder.record_action_failed(action_id, "disconnected");

        assert_eq!(
            receiver.try_recv().unwrap(),
            ApplicationEvent::ActionRequested {
                action_id,
                origin: ProcessActionOrigin::Lua,
                action,
            },
        );
        assert_eq!(
            receiver.try_recv().unwrap(),
            ApplicationEvent::ActionFailed {
                action_id,
                error: "disconnected".to_owned(),
            },
        );
    }

    #[test]
    fn publishes_measurements_to_application_events() {
        let events = crate::application_event::ApplicationEventHub::new();
        let receiver = events.subscribe();
        let recorder =
            ProcessRecorder::spawn_with_events(super::NullProcessRecordWriter, events).unwrap();
        let measurement = super::ProcessMeasurement {
            connection_id: ConnectionId::PRIMARY,
            series_id: SeriesId::new(3),
            series_name: "temperature".to_owned(),
            timestamp: 12.5,
            value: 123.0,
        };

        recorder.record(ProcessRecord::Measurements {
            measurements: vec![measurement.clone()],
        });

        assert_eq!(
            receiver.try_recv().unwrap(),
            ApplicationEvent::Measurements(vec![measurement]),
        );
    }
}
