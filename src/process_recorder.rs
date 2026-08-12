use std::{path::PathBuf, sync::Arc, time::SystemTime};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessLogLevel {
    Info,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
