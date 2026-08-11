use std::path::Path;

use crossbeam_channel::Receiver;

use crate::{
    components::device_emulator_model::DeviceEmulatorModel, data::SeriesId,
    user_command::UserCommand,
};

mod acquisition_controller;
mod command_dispatcher;

pub(crate) use acquisition_controller::{AcquisitionController, RecordingTransition};

pub(crate) use command_dispatcher::CommandDispatcher;

pub struct ApplicationRuntime {
    acquisition: AcquisitionController,
    dispatcher: CommandDispatcher,
    device_emulator: DeviceEmulatorModel,
    lua_command_receiver: Receiver<UserCommand>,
}

impl ApplicationRuntime {
    pub(crate) fn new(
        acquisition: AcquisitionController,
        dispatcher: CommandDispatcher,
        device_emulator: DeviceEmulatorModel,
        lua_command_receiver: Receiver<UserCommand>,
    ) -> Self {
        Self {
            acquisition,
            dispatcher,
            device_emulator,
            lua_command_receiver,
        }
    }

    pub fn poll(&mut self) {
        self.device_emulator.poll();

        self.dispatcher.poll_events(&mut self.acquisition);

        let commands = self.lua_command_receiver.try_iter().collect::<Vec<_>>();

        for command in commands {
            self.execute(command);
        }
    }

    pub fn execute(&mut self, command: UserCommand) {
        self.dispatcher
            .execute(command, &mut self.acquisition, &mut self.device_emulator);
    }

    pub fn is_running(&self) -> bool {
        self.acquisition.is_running()
    }

    pub fn is_recording(&self) -> bool {
        self.acquisition.is_recording()
    }

    pub fn recording_transition(&self) -> Option<RecordingTransition> {
        self.acquisition.recording_transition()
    }

    pub fn recording_file(&self) -> Option<&Path> {
        self.acquisition.recording_file()
    }

    pub fn recording_error(&self) -> Option<&str> {
        self.acquisition.recording_error()
    }

    pub fn set_series_visibility(&self, id: SeriesId, visible: bool) {
        self.dispatcher.set_visibility(id, visible);
    }

    pub fn remove_series(&self, id: SeriesId) {
        self.dispatcher.remove_series(id);
    }
}
