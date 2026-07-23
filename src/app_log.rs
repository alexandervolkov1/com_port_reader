use std::{
    collections::VecDeque,
    fs::{self, File, OpenOptions},
    io::{self, BufWriter, Write},
    path::Path,
};

use chrono::{Local, NaiveDate};
use crossbeam_channel::{Receiver, Sender, unbounded};

const MAX_LOG_ENTRIES: usize = 2_000;
const LOG_DIRECTORY: &str = "logs";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Error,
}

#[derive(Clone, Debug)]
pub struct LogEntry {
    level: LogLevel,
    text: String,
}

impl LogEntry {
    fn new(level: LogLevel, message: impl AsRef<str>) -> Self {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");

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

struct LogMessage {
    level: LogLevel,
    text: String,
}

#[derive(Clone)]
pub struct LogHandle {
    sender: Sender<LogMessage>,
}

impl LogHandle {
    pub fn info(&self, message: impl Into<String>) {
        self.send(LogLevel::Info, message);
    }

    pub fn error(&self, message: impl Into<String>) {
        self.send(LogLevel::Error, message);
    }

    fn send(&self, level: LogLevel, message: impl Into<String>) {
        let _ = self.sender.send(LogMessage {
            level,
            text: message.into(),
        });
    }
}

pub struct LogModel {
    receiver: Receiver<LogMessage>,
    entries: VecDeque<LogEntry>,
    file_writer: LogFileWriter,
    file_logging_enabled: bool,
}

impl LogModel {
    pub fn new() -> (Self, LogHandle) {
        let (sender, receiver) = unbounded();

        (
            Self {
                receiver,
                entries: VecDeque::new(),
                file_writer: LogFileWriter::default(),
                file_logging_enabled: true,
            },
            LogHandle { sender },
        )
    }

    pub fn poll(&mut self) {
        while let Ok(message) = self.receiver.try_recv() {
            let entry = LogEntry::new(message.level, message.text);

            let write_result = if self.file_logging_enabled {
                self.file_writer.write(entry.text())
            } else {
                Ok(())
            };

            self.push_entry(entry);

            if let Err(error) = write_result {
                self.file_logging_enabled = false;

                self.push_entry(LogEntry::new(
                    LogLevel::Error,
                    format!(
                        "Application log file disabled: \
                         {error}",
                    ),
                ));
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
}

#[derive(Default)]
struct LogFileWriter {
    current_file: Option<CurrentLogFile>,
}

struct CurrentLogFile {
    date: NaiveDate,
    writer: BufWriter<File>,
}

impl LogFileWriter {
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
        let directory = Path::new(LOG_DIRECTORY);

        fs::create_dir_all(directory)?;

        let path = directory.join(format!("application {}.log", date.format("%Y-%m-%d"),));

        let file = OpenOptions::new().create(true).append(true).open(path)?;

        self.current_file = Some(CurrentLogFile {
            date,
            writer: BufWriter::new(file),
        });

        Ok(())
    }
}
