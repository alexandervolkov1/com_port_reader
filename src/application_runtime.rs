use std::path::Path;

use crossbeam_channel::Receiver;

use crate::{
    components::{
        controls_model::{ControlsModel, RecordingTransition},
        device_emulator_model::DeviceEmulatorModel,
    },
    data::SeriesId,
    user_command::UserCommand,
};

mod command_dispatcher;

pub(crate) use command_dispatcher::CommandDispatcher;

pub struct ApplicationRuntime {
    controls: ControlsModel,
    dispatcher: CommandDispatcher,
    device_emulator: DeviceEmulatorModel,
    lua_command_receiver: Receiver<UserCommand>,
}

impl ApplicationRuntime {
    pub(crate) fn new(
        controls: ControlsModel,
        dispatcher: CommandDispatcher,
        device_emulator: DeviceEmulatorModel,
        lua_command_receiver: Receiver<UserCommand>,
    ) -> Self {
        Self {
            controls,
            dispatcher,
            device_emulator,
            lua_command_receiver,
        }
    }

    pub fn poll(&mut self) {
        self.device_emulator.poll();

        self.dispatcher.poll_events(&mut self.controls);

        let commands = self.lua_command_receiver.try_iter().collect::<Vec<_>>();

        for command in commands {
            self.execute(command);
        }
    }

    pub fn execute(&mut self, command: UserCommand) {
        self.dispatcher
            .execute(command, &mut self.controls, &mut self.device_emulator);
    }

    pub fn is_running(&self) -> bool {
        self.controls.is_running()
    }

    pub fn is_recording(&self) -> bool {
        self.controls.is_recording()
    }

    pub fn recording_transition(&self) -> Option<RecordingTransition> {
        self.controls.recording_transition()
    }

    pub fn recording_file(&self) -> Option<&Path> {
        self.controls.recording_file()
    }

    pub fn recording_error(&self) -> Option<&str> {
        self.controls.recording_error()
    }

    pub fn set_series_visibility(&self, id: SeriesId, visible: bool) {
        self.dispatcher.set_visibility(id, visible);
    }

    pub fn remove_series(&self, id: SeriesId) {
        self.dispatcher.remove_series(id);
    }
}
