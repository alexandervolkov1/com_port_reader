use std::collections::VecDeque;

use chrono::Local;
use crossbeam_channel::{Receiver, Sender};

const MAX_VISIBLE_ENTRIES: usize = 2_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Error,
}

impl LogLevel {
    const fn label(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Error => "ERROR",
        }
    }
}

pub struct LogEntry {
    level: LogLevel,
    text: String,
}

impl LogEntry {
    fn new(level: LogLevel, message: impl Into<String>) -> Self {
        let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");

        let message = message.into();

        Self {
            level,
            text: format!("[{timestamp}] {:<5} {message}", level.label(),),
        }
    }

    pub const fn level(&self) -> LogLevel {
        self.level
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

#[derive(Clone)]
pub struct LogHandle {
    sender: Sender<LogEntry>,
}

impl LogHandle {
    pub fn info(&self, message: impl Into<String>) {
        self.send(LogLevel::Info, message);
    }

    pub fn error(&self, message: impl Into<String>) {
        self.send(LogLevel::Error, message);
    }

    fn send(&self, level: LogLevel, message: impl Into<String>) {
        let _ = self.sender.send(LogEntry::new(level, message));
    }
}

pub struct LogModel {
    receiver: Receiver<LogEntry>,
    entries: VecDeque<LogEntry>,
}

impl LogModel {
    pub fn new() -> (Self, LogHandle) {
        let (sender, receiver) = crossbeam_channel::unbounded();

        let model = Self {
            receiver,
            entries: VecDeque::new(),
        };

        let handle = LogHandle { sender };

        (model, handle)
    }

    pub fn poll(&mut self) {
        while let Ok(entry) = self.receiver.try_recv() {
            if self.entries.len() >= MAX_VISIBLE_ENTRIES {
                self.entries.pop_front();
            }

            self.entries.push_back(entry);
        }
    }

    pub fn entries(&self) -> &VecDeque<LogEntry> {
        &self.entries
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
