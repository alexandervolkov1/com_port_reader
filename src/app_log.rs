use std::{
    collections::{HashMap, VecDeque},
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::PathBuf,
    time::SystemTime,
};

use chrono::{Local, NaiveDate};
use crossbeam_channel::Receiver;

use crate::process_recorder::{
    ProcessAction, ProcessActionId, ProcessActionResult, ProcessRecord, ProcessRecorder,
};

pub use crate::process_recorder::ProcessLogLevel as LogLevel;

const MAX_LOG_ENTRIES: usize = 2_000;

#[derive(Clone, Debug)]
pub struct LogEntry {
    level: LogLevel,
    text: String,
}

impl LogEntry {
    fn new(timestamp: SystemTime, level: LogLevel, message: impl AsRef<str>) -> Self {
        let timestamp: chrono::DateTime<Local> = timestamp.into();

        let timestamp = timestamp.format("%Y-%m-%d %H:%M:%S%.3f");

        Self {
            level,
            text: format!("[{timestamp}] {}", message.as_ref(),),
        }
    }

    pub fn level(&self) -> LogLevel {
        self.level
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone)]
pub struct LogHandle {
    recorder: ProcessRecorder,
}

impl LogHandle {
    pub fn info(&self, message: impl Into<String>) {
        self.send(LogLevel::Info, message);
    }

    pub fn error(&self, message: impl Into<String>) {
        self.send(LogLevel::Error, message);
    }

    fn send(&self, level: LogLevel, message: impl Into<String>) {
        self.recorder.record(ProcessRecord::Log {
            timestamp: SystemTime::now(),
            level,
            message: message.into(),
        });
    }
}

pub struct LogModel {
    receiver: Receiver<ProcessRecord>,
    entries: VecDeque<LogEntry>,
    pending_actions: HashMap<ProcessActionId, ProcessAction>,
    file_writer: LogFileWriter,
    file_logging_enabled: bool,
}

impl LogModel {
    pub fn new(log_directory: impl Into<PathBuf>, recorder: ProcessRecorder) -> (Self, LogHandle) {
        let receiver = recorder.subscribe_timeline();

        (
            Self {
                receiver,
                entries: VecDeque::new(),
                pending_actions: HashMap::new(),
                file_writer: LogFileWriter::new(log_directory),
                file_logging_enabled: true,
            },
            LogHandle { recorder },
        )
    }

    pub fn poll(&mut self) {
        while let Ok(record) = self.receiver.try_recv() {
            match record {
                ProcessRecord::Log {
                    timestamp,
                    level,
                    message,
                } => {
                    self.push_message(timestamp, level, message);
                }

                ProcessRecord::ActionRequested {
                    action_id, action, ..
                } => {
                    self.pending_actions.insert(action_id, action);
                }

                ProcessRecord::ActionApplied {
                    action_id,
                    timestamp,
                    series_id,
                    series_name,
                    result,
                } => {
                    let message = self
                        .pending_actions
                        .remove(&action_id)
                        .map(|action| {
                            action_applied_message(
                                &action,
                                series_id,
                                series_name.as_deref(),
                                result.as_ref(),
                            )
                        })
                        .unwrap_or_else(|| format!("Action {} completed.", action_id.value(),));

                    self.push_message(timestamp, LogLevel::Info, message);
                }

                ProcessRecord::ActionFailed {
                    action_id,
                    timestamp,
                    error,
                } => {
                    let message = self
                        .pending_actions
                        .remove(&action_id)
                        .map(|action| format!("{} failed: {error}", action_description(&action,),))
                        .unwrap_or_else(|| {
                            format!(
                                "Action {} failed: \
                                 {error}",
                                action_id.value(),
                            )
                        });

                    self.push_message(timestamp, LogLevel::Error, message);
                }

                ProcessRecord::ConfigurationLoaded { .. }
                | ProcessRecord::Measurements { .. }
                | ProcessRecord::ControlOutput { .. } => {}
            }
        }
    }

    pub fn entries(&self) -> &VecDeque<LogEntry> {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    fn push_entry(&mut self, entry: LogEntry) {
        if self.entries.len() >= MAX_LOG_ENTRIES {
            self.entries.pop_front();
        }

        self.entries.push_back(entry);
    }

    fn push_message(&mut self, timestamp: SystemTime, level: LogLevel, message: impl AsRef<str>) {
        let entry = LogEntry::new(timestamp, level, message);

        let write_result = if self.file_logging_enabled {
            self.file_writer.write(entry.text())
        } else {
            Ok(())
        };

        self.push_entry(entry);

        if let Err(error) = write_result {
            self.file_logging_enabled = false;

            self.push_entry(LogEntry::new(
                SystemTime::now(),
                LogLevel::Error,
                format!(
                    "Application log file disabled: \
                     {error}",
                ),
            ));
        }
    }
}

fn action_description(action: &ProcessAction) -> String {
    match action {
        ProcessAction::StartAcquisition => "Acquisition start".to_owned(),

        ProcessAction::StopAcquisition => "Acquisition stop".to_owned(),

        ProcessAction::ClearSeries => "Clear series".to_owned(),

        ProcessAction::StartEmulator => "Device emulator start".to_owned(),

        ProcessAction::StopEmulator => "Device emulator stop".to_owned(),

        ProcessAction::AddSeries { name, .. } => name.as_ref().map_or_else(
            || "Add series".to_owned(),
            |name| format!("Add series '{name}'",),
        ),

        ProcessAction::AddFilteredSeries { name, .. } => {
            format!("Add filtered series '{name}'",)
        }

        ProcessAction::AddController { name, .. } => {
            format!("Add controller '{name}'")
        }

        ProcessAction::WriteControllerParameter { name, key, .. } => {
            format!(
                "Write controller '{name}' \
                 parameter '{key}'",
            )
        }

        ProcessAction::ConfigureController { name, .. } => {
            format!("Configure controller '{name}'",)
        }

        ProcessAction::WriteControllerReferenceParameter { name, key, .. } => {
            format!(
                "Write controller '{name}' \
                 reference parameter '{key}'",
            )
        }

        ProcessAction::ConfigureControllerReference { name, .. } => {
            format!(
                "Configure controller '{name}' \
                 reference",
            )
        }

        ProcessAction::SetControllerReference { name, .. } => {
            format!(
                "Set controller '{name}' \
                 reference",
            )
        }

        ProcessAction::SetControllerInput { name, .. } => {
            format!("Set controller '{name}' input",)
        }

        ProcessAction::PauseController { name } => {
            format!("Pause controller '{name}'")
        }

        ProcessAction::ResumeController { name } => {
            format!("Resume controller '{name}'")
        }

        ProcessAction::ResetControllerIntegral { name } => {
            format!(
                "Reset controller '{name}' \
                 integral",
            )
        }

        ProcessAction::ResetController { name } => {
            format!("Reset controller '{name}'")
        }

        ProcessAction::SetFilter { name, .. } => {
            format!(
                "Change filter for series \
                 '{name}'",
            )
        }

        ProcessAction::DeleteSeriesByName { name, .. } => {
            format!("Delete series '{name}'")
        }

        ProcessAction::RenameSeries {
            current_name,
            new_name,
            ..
        } => {
            format!(
                "Rename series '{current_name}' \
                 to '{new_name}'",
            )
        }

        ProcessAction::SetSeriesColor { name, .. } => {
            format!("Change series '{name}' color",)
        }

        ProcessAction::SetSeriesVisibility {
            series_name,
            series_id,
            ..
        } => match series_name {
            Some(name) => {
                format!(
                    "Change series '{name}' \
                     visibility",
                )
            }

            None => {
                format!(
                    "Change series {series_id} \
                     visibility",
                )
            }
        },

        ProcessAction::SendSerial { connection_id, .. } => {
            format!(
                "Send serial command on \
                 connection {connection_id}",
            )
        }

        ProcessAction::ReadInstrument { connection_id, .. } => {
            format!(
                "Read instrument on connection \
                 {connection_id}",
            )
        }

        ProcessAction::WriteInstrument { connection_id, .. } => {
            format!(
                "Write instrument on connection \
                 {connection_id}",
            )
        }

        ProcessAction::DescribeVirtualInstruments { connection_id } => {
            format!(
                "Describe virtual instruments on \
                 connection {connection_id}",
            )
        }
    }
}

fn action_applied_message(
    action: &ProcessAction,
    series_id: Option<crate::data::SeriesId>,
    series_name: Option<&str>,
    result: Option<&ProcessActionResult>,
) -> String {
    match action {
        ProcessAction::SendSerial {
            connection_id,
            command,
        } => match result {
            Some(ProcessActionResult::SerialResponse(response)) => {
                format!(
                    "Serial command '{command}' on \
                         connection {connection_id} \
                         returned: {response}"
                )
            }

            _ => {
                format!(
                    "Serial command '{command}' on \
                         connection {connection_id} \
                         completed."
                )
            }
        },

        ProcessAction::ReadInstrument {
            connection_id,
            request,
        } => match result {
            Some(ProcessActionResult::InstrumentValue(value)) => {
                format!(
                    "Instrument read on connection \
                         {connection_id}: {request} \
                         returned {value}."
                )
            }

            _ => {
                format!(
                    "Instrument read on connection \
                         {connection_id} completed."
                )
            }
        },

        ProcessAction::WriteInstrument {
            connection_id,
            request,
        } => match result {
            Some(ProcessActionResult::InstrumentValue(value)) => {
                format!(
                    "Instrument write on connection \
                         {connection_id}: {request}; \
                         actual value: {value}."
                )
            }

            _ => {
                format!(
                    "Instrument write on connection \
                         {connection_id} completed."
                )
            }
        },

        ProcessAction::DescribeVirtualInstruments { connection_id } => match result {
            Some(ProcessActionResult::VirtualInstrumentCount(count)) => {
                format!(
                    "Virtual instrument discovery on \
                         connection {connection_id} \
                         returned {count} descriptor(s)."
                )
            }

            _ => {
                format!(
                    "Virtual instrument discovery on \
                         connection {connection_id} \
                         completed."
                )
            }
        },

        ProcessAction::AddSeries { .. } | ProcessAction::AddFilteredSeries { .. } => {
            match (series_name, series_id) {
                (Some(name), Some(id)) => {
                    format!("Series '{name}' ({id}) added.",)
                }

                (Some(name), None) => {
                    format!("Series '{name}' added.",)
                }

                _ => {
                    format!("{} completed.", action_description(action),)
                }
            }
        }

        ProcessAction::DeleteSeriesByName { name, .. } => {
            format!("Series '{name}' removed.")
        }

        ProcessAction::RenameSeries { new_name, .. } => match series_id {
            Some(id) => {
                format!(
                    "Series {id} renamed to \
                     '{new_name}'.",
                )
            }

            None => {
                format!(
                    "Series renamed to \
                     '{new_name}'.",
                )
            }
        },

        _ => {
            format!("{} completed.", action_description(action),)
        }
    }
}

struct LogFileWriter {
    directory: PathBuf,
    current_file: Option<CurrentLogFile>,
}

impl LogFileWriter {
    fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
            current_file: None,
        }
    }

    fn write(&mut self, text: &str) -> io::Result<()> {
        let current_date = Local::now().date_naive();

        let open_new_file = self
            .current_file
            .as_ref()
            .is_none_or(|file| file.date != current_date);

        if open_new_file {
            self.open_file(current_date)?;
        }

        let file = self.current_file.as_mut().expect("log file must be open");

        writeln!(file.writer, "{text}")?;

        file.writer.flush()
    }

    fn open_file(&mut self, date: NaiveDate) -> io::Result<()> {
        fs::create_dir_all(&self.directory)?;

        let path = self
            .directory
            .join(format!("application {}.log", date.format("%Y-%m-%d"),));

        let file = OpenOptions::new().create(true).append(true).open(path)?;

        self.current_file = Some(CurrentLogFile {
            date,
            writer: BufWriter::new(file),
        });

        Ok(())
    }
}

struct CurrentLogFile {
    date: NaiveDate,
    writer: BufWriter<File>,
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    use crate::connection::ConnectionId;
    use crate::process_recorder::ProcessActionResult;
    use crate::process_recorder::{ProcessAction, ProcessActionOrigin, ProcessRecorder};

    use super::LogFileWriter;

    static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn writes_to_configured_directory() {
        let directory = std::env::temp_dir().join(format!(
            "com_port_reader_log_test_{}_{}",
            std::process::id(),
            NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed,),
        ));

        let mut writer = LogFileWriter::new(&directory);

        writer.write("test entry").unwrap();

        drop(writer);

        let files = fs::read_dir(&directory)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert_eq!(files.len(), 1);

        let contents = fs::read_to_string(files[0].path()).unwrap();

        assert_eq!(contents, "test entry\n");

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn renders_completed_action_from_timeline() {
        let directory = std::env::temp_dir().join(format!(
            "com_port_reader_timeline_test_{}_{}",
            std::process::id(),
            NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed,),
        ));

        let recorder = ProcessRecorder::default();

        let (mut model, _log) = super::LogModel::new(&directory, recorder.clone());

        let action_id = recorder.record_action(
            ProcessActionOrigin::UserInterface,
            ProcessAction::StartAcquisition,
        );

        recorder.record_action_applied(action_id, None, None);

        model.poll();

        assert_eq!(model.entries().len(), 1);

        assert!(
            model.entries()[0]
                .text()
                .contains("Acquisition start completed.",),
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn renders_serial_action_result() {
        let action = ProcessAction::SendSerial {
            connection_id: ConnectionId::PRIMARY,
            command: "get".to_owned(),
        };

        let result = ProcessActionResult::SerialResponse("42".to_owned());

        let message = super::action_applied_message(&action, None, None, Some(&result));

        assert!(message.contains("Serial command 'get'",));

        assert!(message.contains("returned: 42"));
    }

    #[test]
    fn ordinary_log_is_rendered_once() {
        let directory = std::env::temp_dir().join(format!(
            "com_port_reader_plain_log_test_{}_{}",
            std::process::id(),
            NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed,),
        ));

        let recorder = ProcessRecorder::default();

        let (mut model, log) = super::LogModel::new(&directory, recorder);

        log.info("Test message");

        model.poll();

        assert_eq!(model.entries().len(), 1,);

        assert!(model.entries()[0].text().contains("Test message"),);

        fs::remove_dir_all(directory).unwrap();
    }
}
