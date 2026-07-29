use crossbeam_channel::{Receiver, TryRecvError};

use crate::{
    app_log::LogHandle,
    lua_worker::{LuaEvent, LuaWorkerHandle},
};

pub struct LuaConsoleModel {
    worker: LuaWorkerHandle,
    event_receiver: Receiver<LuaEvent>,
    command_buffer: String,
    pending: bool,
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
            pending: false,
            disconnected: false,
            focus_requested: false,
            log,
        }
    }

    pub fn command_buffer_mut(&mut self) -> &mut String {
        &mut self.command_buffer
    }

    pub fn is_pending(&self) -> bool {
        self.pending
    }

    pub fn is_available(&self) -> bool {
        !self.pending && !self.disconnected
    }

    pub fn take_focus_request(&mut self) -> bool {
        std::mem::take(&mut self.focus_requested)
    }

    pub fn submit(&mut self) {
        if !self.is_available() || self.command_buffer.trim().is_empty() {
            return;
        }

        match self.worker.execute(self.command_buffer.clone()) {
            Ok(()) => {
                self.pending = true;
            }

            Err(error) => {
                self.disconnected = true;
                self.focus_requested = true;

                self.log.error(format!(
                    "Failed to submit Lua command: \
                     {error}",
                ));
            }
        }
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
                        self.disconnected = true;
                        self.pending = false;
                        self.focus_requested = true;

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

    fn handle_event(&mut self, event: LuaEvent) {
        self.pending = false;
        self.focus_requested = true;

        match event {
            LuaEvent::ExecutionSucceeded(output) => {
                self.command_buffer.clear();

                if output.is_empty() {
                    self.log.info("Lua command executed.");
                } else {
                    self.log.info(format!("Lua result: {}", output.join("\t"),));
                }
            }

            LuaEvent::ExecutionFailed(error) => {
                self.log.error(format!("Lua error: {error}",));
            }
        }
    }
}
