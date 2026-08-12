use std::{
    fs,
    path::{Path, PathBuf},
};

use crossbeam_channel::{Receiver, TryRecvError};

use crate::{
    app_log::LogHandle,
    lua_worker::{LuaEvent, LuaWorkerHandle},
};

enum PendingExecution {
    Console { source: String },

    File(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LuaTranscriptEntry {
    Command(String),
    Result(String),
    Error(String),
}

pub struct LuaConsoleModel {
    worker: LuaWorkerHandle,
    event_receiver: Receiver<LuaEvent>,
    command_buffer: String,
    transcript: Vec<LuaTranscriptEntry>,
    pending: Option<PendingExecution>,
    disconnected: bool,
    focus_requested: bool,
    open: bool,
    log: LogHandle,
    script_directory: PathBuf,
}

impl LuaConsoleModel {
    pub fn new(
        worker: LuaWorkerHandle,
        event_receiver: Receiver<LuaEvent>,
        script_directory: impl Into<PathBuf>,
        log: LogHandle,
    ) -> Self {
        Self {
            worker,
            event_receiver,
            command_buffer: String::new(),
            transcript: Vec::new(),
            pending: None,
            disconnected: false,
            focus_requested: false,
            open: false,
            script_directory: script_directory.into(),
            log,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn toggle_open(&mut self) {
        self.open = !self.open;
    }

    pub fn command_buffer_mut(&mut self) -> &mut String {
        &mut self.command_buffer
    }

    pub fn transcript(&self) -> &[LuaTranscriptEntry] {
        &self.transcript
    }

    pub fn clear_transcript(&mut self) {
        self.transcript.clear();
    }

    pub fn can_submit(&self) -> bool {
        self.is_available() && !self.command_buffer.trim().is_empty()
    }

    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn is_available(&self) -> bool {
        self.pending.is_none() && !self.disconnected
    }

    pub fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_requested)
    }

    pub fn submit(&mut self) {
        if !self.can_submit() {
            return;
        }

        let source = self.command_buffer.trim().to_owned();

        self.transcript
            .push(LuaTranscriptEntry::Command(source.clone()));

        self.submit_source(source.clone(), PendingExecution::Console { source });
    }

    pub fn run_file(&mut self, path: &Path) {
        if !self.is_available() {
            return;
        }

        let source = match fs::read_to_string(path) {
            Ok(source) => source,

            Err(error) => {
                self.log.error(format!(
                    "Failed to read Lua script \
                         '{}': {error}",
                    path.display(),
                ));

                return;
            }
        };

        self.submit_source(source, PendingExecution::File(path.to_path_buf()));
    }

    pub fn poll_events(&mut self) {
        loop {
            match self.event_receiver.try_recv() {
                Ok(event) => {
                    self.handle_event(event);
                }

                Err(TryRecvError::Empty) => {
                    break;
                }

                Err(TryRecvError::Disconnected) => {
                    self.handle_disconnection();

                    break;
                }
            }
        }
    }

    fn submit_source(&mut self, source: String, origin: PendingExecution) {
        let console_execution = matches!(&origin, PendingExecution::Console { .. });

        match self.worker.execute(source) {
            Ok(()) => {
                if console_execution {
                    self.command_buffer.clear();
                }

                self.pending = Some(origin);
            }

            Err(error) => {
                self.disconnected = true;
                self.focus_requested = console_execution;

                match origin {
                    PendingExecution::Console { .. } => {
                        let message = format!(
                            "Failed to submit Lua \
                             command: {error}",
                        );

                        self.transcript
                            .push(LuaTranscriptEntry::Error(message.clone()));

                        self.log.error(message);
                    }

                    PendingExecution::File(path) => {
                        self.log.error(format!(
                            "Failed to submit Lua \
                             script '{}': {error}",
                            path.display(),
                        ));
                    }
                }
            }
        }
    }

    fn handle_disconnection(&mut self) {
        if self.disconnected {
            return;
        }

        let pending = self.pending.take();

        self.disconnected = true;

        let message = "Lua worker event channel \
             is disconnected."
            .to_owned();

        match pending {
            Some(PendingExecution::Console { source }) => {
                if self.command_buffer.is_empty() {
                    self.command_buffer = source;
                }

                self.focus_requested = true;

                self.transcript
                    .push(LuaTranscriptEntry::Error(message.clone()));
            }

            Some(PendingExecution::File(_)) | None => {}
        }

        self.log.error(message);
    }

    fn handle_event(&mut self, event: LuaEvent) {
        let pending = self.pending.take();

        self.focus_requested = matches!(&pending, Some(PendingExecution::Console { .. },),);

        match event {
            LuaEvent::ExecutionSucceeded(output) => {
                self.handle_success(pending, output);
            }

            LuaEvent::ExecutionFailed(error) => {
                self.handle_failure(pending, error);
            }

            LuaEvent::InitializationFailed(error) => {
                self.disconnected = true;
                self.focus_requested = true;

                let message = format!(
                    "Failed to initialize Lua \
                     runtime: {error}",
                );

                self.transcript
                    .push(LuaTranscriptEntry::Error(message.clone()));

                self.log.error(message);
            }
        }
    }

    fn handle_success(&mut self, pending: Option<PendingExecution>, output: Vec<String>) {
        match pending {
            Some(PendingExecution::Console { .. }) if !output.is_empty() => {
                self.transcript
                    .push(LuaTranscriptEntry::Result(output.join("\t")));
            }

            Some(PendingExecution::Console { .. }) => {}

            Some(PendingExecution::File(path)) => {
                self.log.info(format!(
                    "Lua script '{}' executed successfully.",
                    path.display(),
                ));
            }

            None => {}
        }
    }

    fn handle_failure(&mut self, pending: Option<PendingExecution>, error: String) {
        match pending {
            Some(PendingExecution::Console { source }) => {
                if self.command_buffer.is_empty() {
                    self.command_buffer = source;
                }

                self.transcript.push(LuaTranscriptEntry::Error(error));
            }

            Some(PendingExecution::File(path)) => {
                self.log.error(format!(
                    "Lua script '{}' failed: \
                     {error}",
                    path.display(),
                ));
            }

            None => {
                self.transcript.push(LuaTranscriptEntry::Error(error));
            }
        }
    }

    pub fn script_directory(&self) -> &Path {
        &self.script_directory
    }

    pub fn replace_worker(&mut self, worker: LuaWorkerHandle, event_receiver: Receiver<LuaEvent>) {
        debug_assert!(
            self.pending.is_none(),
            "pending Lua execution must finish before \
             replacing its worker",
        );

        self.worker = worker;
        self.event_receiver = event_receiver;
        self.pending = None;
        self.disconnected = false;
        self.focus_requested = true;
    }
}
