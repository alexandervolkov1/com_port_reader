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
    Console,
    File(PathBuf),
}

pub struct LuaConsoleModel {
    worker: LuaWorkerHandle,
    event_receiver: Receiver<LuaEvent>,
    command_buffer: String,
    pending: Option<PendingExecution>,
    disconnected: bool,
    focus_requested: bool,
    log: LogHandle,
}

impl LuaConsoleModel {
    pub fn new(
        worker: LuaWorkerHandle,
        event_receiver: Receiver<LuaEvent>,
        log: LogHandle,
    ) -> Self {
        Self {
            worker,
            event_receiver,
            command_buffer: String::new(),
            pending: None,
            disconnected: false,
            focus_requested: false,
            log,
        }
    }

    pub fn command_buffer_mut(&mut self) -> &mut String {
        &mut self.command_buffer
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
        if !self.is_available() || self.command_buffer.trim().is_empty() {
            return;
        }

        self.submit_source(self.command_buffer.clone(), PendingExecution::Console);
    }

    pub fn run_file(&mut self, path: &Path) {
        if !self.is_available() {
            return;
        }

        let source = match fs::read_to_string(path) {
            Ok(source) => source,

            Err(error) => {
                self.log.error(format!(
                    "Failed to read Lua script '{}': \
                     {error}",
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
                    if !self.disconnected {
                        let pending = self.pending.take();

                        self.disconnected = true;

                        self.focus_requested = matches!(&pending, Some(PendingExecution::Console),);

                        self.log.error(
                            "Lua worker event channel \
                             is disconnected.",
                        );
                    }

                    break;
                }
            }
        }
    }

    fn submit_source(&mut self, source: String, origin: PendingExecution) {
        let focus_on_error = matches!(&origin, PendingExecution::Console,);

        match self.worker.execute(source) {
            Ok(()) => {
                self.pending = Some(origin);
            }

            Err(error) => {
                self.disconnected = true;
                self.focus_requested = focus_on_error;

                match origin {
                    PendingExecution::Console => {
                        self.log.error(format!(
                            "Failed to submit Lua \
                             command: {error}",
                        ));
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

    fn handle_event(&mut self, event: LuaEvent) {
        let pending = self.pending.take();

        self.focus_requested = matches!(&pending, Some(PendingExecution::Console),);

        match event {
            LuaEvent::ExecutionSucceeded(output) => {
                match pending {
                    Some(PendingExecution::Console) => {
                        self.command_buffer.clear();
                    }

                    Some(PendingExecution::File(path)) => {
                        self.log.info(format!(
                            "Lua script '{}' \
                             executed successfully.",
                            path.display(),
                        ));
                    }

                    None => {}
                }

                if !output.is_empty() {
                    self.log.info(format!("Lua result: {}", output.join("\t"),));
                }
            }

            LuaEvent::ExecutionFailed(error) => match pending {
                Some(PendingExecution::File(path)) => {
                    self.log.error(format!(
                        "Lua script '{}' failed: \
                             {error}",
                        path.display(),
                    ));
                }

                Some(PendingExecution::Console) | None => {
                    self.log.error(format!("Lua error: {error}",));
                }
            },

            LuaEvent::InitializationFailed(error) => {
                self.disconnected = true;
                self.focus_requested = true;

                self.log.error(format!(
                    "Failed to initialize Lua \
                     runtime: {error}",
                ));
            }
        }
    }
}
